use core::mem::{offset_of, MaybeUninit};
use core::num::NonZero;
use core::ptr::{self, NonNull};

use static_assertions::{const_assert, const_assert_eq};
#[cfg(feature = "tls")]
use {
	crate::emma::{AtomicHeapId, HeapId},
	core::sync::atomic::{AtomicU32, Ordering},
};

use crate::mmap::alloc_aligned;

const ARENA_SIZE: u32 = 4 * 1024 * 1024;
const PAGE_SIZE: u32 = 32 * 1024;
const PAGES_PER_ARENA: u32 = ARENA_SIZE / PAGE_SIZE;
pub const MAXIMUM_OBJECT_ALIGNMENT: u32 = 256;
const METADATA_ZONE_SIZE: u32 =
	(size_of::<Arena>() as u32 + MAXIMUM_OBJECT_ALIGNMENT - 1) & !(MAXIMUM_OBJECT_ALIGNMENT - 1);

#[derive(Debug)]
struct Arena {
	#[cfg(feature = "tls")]
	owner: AtomicHeapId,
	pages: [Page; PAGES_PER_ARENA as usize],
}

const_assert!(ARENA_SIZE.is_power_of_two());
const_assert!(PAGE_SIZE.is_power_of_two());
const_assert!(MAXIMUM_OBJECT_ALIGNMENT.is_power_of_two());
const_assert_eq!(ARENA_SIZE % PAGE_SIZE, 0);
const_assert_eq!(PAGE_SIZE % MAXIMUM_OBJECT_ALIGNMENT, 0);
const_assert_eq!(METADATA_ZONE_SIZE % MAXIMUM_OBJECT_ALIGNMENT, 0);
const_assert!(size_of::<Arena>() > (METADATA_ZONE_SIZE - MAXIMUM_OBJECT_ALIGNMENT) as usize);
const_assert!(size_of::<Arena>() <= METADATA_ZONE_SIZE as usize);

impl Arena {
	/// Makes a pointer to the arena from any pointer to a location inside the arena.
	#[inline]
	unsafe fn from_inner_ptr(p: NonNull<u8>) -> NonNull<Arena> {
		NonNull::new_unchecked(((p.as_ptr() as usize) & !(ARENA_SIZE as usize - 1)) as *mut Arena)
	}

	#[inline]
	unsafe fn object_offset(p: NonNull<u8>) -> NonZero<u32> {
		NonZero::new_unchecked((p.as_ptr() as u32) % ARENA_SIZE)
	}
}

#[derive(Debug)]
pub struct Page {
	pub next_page: Option<NonNull<Page>>,
	/// the index into `arena.pages` that yields this page
	page_number: u32,
	/// the free_list is an arena-relative byte offset
	free_list: Option<NonZero<u32>>,
	/// the free_list is an arena-relative byte offset
	#[cfg(feature = "tls")]
	foreign_free_list: AtomicU32,
	/// the amount of bytes that have not yet been added allocated or added to a `free_list`
	bytes_in_reserve: u32,
}

impl Page {
	#[inline]
	pub unsafe fn from_new_arena(
		#[cfg(feature = "tls")] owner: HeapId,
	) -> Option<(NonNull<Page>, NonNull<Page>, NonNull<Page>)> {
		let region = alloc_aligned(
			NonZero::new(ARENA_SIZE as usize).unwrap(),
			NonZero::new(ARENA_SIZE as usize).unwrap(),
			3,
		)?;

		let pages_p = region.byte_add(offset_of!(Arena, pages)).cast::<Page>();
		let mut pages: [MaybeUninit<Page>; PAGES_PER_ARENA as usize] = MaybeUninit::uninit().assume_init();
		pages[0].write(Page {
			next_page: None,
			page_number: 0,
			free_list: None,
			#[cfg(feature = "tls")]
			foreign_free_list: AtomicU32::new(0),
			bytes_in_reserve: PAGE_SIZE - METADATA_ZONE_SIZE,
		});
		for i in 1..pages.len() - 1 {
			pages[i].write(Page {
				next_page: Some(pages_p.add(i + 1)),
				page_number: i as u32,
				free_list: None,
				#[cfg(feature = "tls")]
				foreign_free_list: AtomicU32::new(0),
				bytes_in_reserve: PAGE_SIZE,
			});
		}
		pages[pages.len() - 1].write(Page {
			next_page: None,
			page_number: (pages.len() - 1) as u32,
			free_list: None,
			#[cfg(feature = "tls")]
			foreign_free_list: AtomicU32::new(0),
			bytes_in_reserve: PAGE_SIZE,
		});

		region.cast().write(Arena {
			#[cfg(feature = "tls")]
			owner: AtomicHeapId::new(owner),
			pages: core::mem::transmute::<[MaybeUninit<Page>; PAGES_PER_ARENA as usize], [Page; PAGES_PER_ARENA as usize]>(
				pages,
			),
		});

		Some((pages_p, pages_p.add(1), pages_p.add(PAGES_PER_ARENA as usize - 1)))
	}

	#[inline]
	unsafe fn page_id(p: *mut u8) -> usize {
		(((p as u32) % ARENA_SIZE) / PAGE_SIZE) as usize
	}

	#[inline]
	unsafe fn is_on_page(&mut self, p: *mut u8) -> bool {
		let start = Arena::from_inner_ptr(NonNull::new(self).unwrap().cast())
			.cast::<u8>()
			.byte_add(self.page_number as usize * PAGE_SIZE as usize);
		let end = start.byte_add(PAGE_SIZE as usize);
		start.as_ptr() <= p && p < end.as_ptr()
	}

	#[inline]
	pub fn alloc(&mut self, object_size: u32) -> Option<NonNull<u8>> {
		if let Some(offset) = self.free_list {
			unsafe {
				let p = Arena::from_inner_ptr(NonNull::new_unchecked(self).cast())
					.byte_add(offset.get() as usize)
					.cast();
				self.free_list = p.cast::<Option<NonZero<u32>>>().read();

				debug_assert!(self.is_on_page(p.as_ptr()));
				Some(p)
			}
		} else {
			#[cfg(feature = "tls")]
			{
				if let Some(offset) = NonZero::new(self.foreign_free_list.swap(0, Ordering::Acquire)) {
					unsafe {
						let p = Arena::from_inner_ptr(NonNull::new_unchecked(self).cast())
							.byte_add(offset.get() as usize)
							.cast();
						self.free_list = p.cast::<Option<NonZero<u32>>>().read();

						debug_assert!(self.is_on_page(p.as_ptr()));
						return Some(p);
					}
				}
			}

			if self.bytes_in_reserve >= object_size {
				unsafe {
					let p = Arena::from_inner_ptr(NonNull::new_unchecked(self).cast())
						.cast::<u8>()
						.byte_add(((self.page_number + 1) * PAGE_SIZE - self.bytes_in_reserve) as usize);
					self.bytes_in_reserve -= object_size;

					if self.bytes_in_reserve % 4096 >= object_size {
						self.bytes_in_reserve -= object_size;
						let mut q = p.byte_add(object_size as usize);
						let mut offset = Arena::object_offset(q);
						self.free_list = Some(offset);

						while self.bytes_in_reserve % 4096 >= object_size {
							self.bytes_in_reserve -= object_size;
							let next = q.byte_add(object_size as usize);
							offset = offset.checked_add(object_size).unwrap_unchecked();
							q.cast::<Option<NonZero<u32>>>().write(Some(offset));
							q = next;
						}
						q.cast::<Option<NonZero<u32>>>().write(None);
					}

					debug_assert!(self.is_on_page(p.as_ptr()));
					return Some(p);
				}
			}

			None
		}
	}

	#[cfg(not(feature = "tls"))]
	#[inline]
	pub unsafe fn dealloc(p: NonNull<u8>) {
		let page = &mut unsafe { Arena::from_inner_ptr(p).as_mut() }.pages[Page::page_id(p.as_ptr())];
		p.cast::<Option<NonZero<u32>>>().write(page.free_list);
		page.free_list = Some(Arena::object_offset(p));
	}

	#[cfg(feature = "tls")]
	#[inline]
	pub unsafe fn dealloc(heap_id: HeapId, p: NonNull<u8>) {
		let arena = Arena::from_inner_ptr(p);
		let mut page = arena
			.byte_add(offset_of!(Arena, pages))
			.cast::<Page>()
			.add(Page::page_id(p.as_ptr()));

		let p_offset = Arena::object_offset(p);

		let owner = arena
			.byte_add(offset_of!(Arena, owner))
			.cast::<AtomicHeapId>()
			.as_ref()
			.load(Ordering::Relaxed);
		if owner == heap_id {
			let page = page.as_mut();
			p.cast::<Option<NonZero<u32>>>().write(page.free_list);
			page.free_list = Some(p_offset);
		} else {
			let free_list = page
				.byte_add(offset_of!(Page, foreign_free_list))
				.cast::<AtomicU32>()
				.as_ref();
			let mut next = free_list.load(Ordering::Relaxed);
			loop {
				p.cast::<Option<NonZero<u32>>>().write(NonZero::new(next));
				match free_list.compare_exchange(next, p_offset.get(), Ordering::Release, Ordering::Relaxed) {
					Ok(_) => break,
					Err(new_next) => next = new_next,
				}
			}
		}
	}
}

#[inline]
pub unsafe fn alloc(
	bin: &mut Option<NonNull<Page>>,
	reserve_pages: &mut Option<NonNull<Page>>,
	object_size: u32,
	#[cfg(feature = "tls")] id: HeapId,
) -> *mut u8 {
	{
		let mut pp: *mut Option<NonNull<Page>> = bin;
		let mut p = *bin;
		while let Some(mut q) = p {
			let page = q.as_mut();

			if let Some(ret) = page.alloc(object_size) {
				if p != *bin {
					*pp.as_mut().unwrap_unchecked() = page.next_page;
					page.next_page = *bin;
					*bin = p;
				}
				return ret.as_ptr();
			}
			pp = &mut page.next_page;
			p = page.next_page;
		}
	}

	if let Some(mut p) = *reserve_pages {
		let page = p.as_mut();

		*reserve_pages = page.next_page;
		page.next_page = *bin;
		*bin = Some(p);

		let ret = page.alloc(object_size);
		debug_assert!(ret.is_some());
		return unsafe { ret.unwrap_unchecked() }.as_ptr();
	}

	#[cfg(not(feature = "tls"))]
	let pages_from_new_arena = Page::from_new_arena();
	#[cfg(feature = "tls")]
	let pages_from_new_arena = Page::from_new_arena(id);
	if let Some((mut page, first_additional_page, mut last_additional_page)) = pages_from_new_arena {
		debug_assert_eq!(last_additional_page.as_ref().next_page, None);
		last_additional_page.as_mut().next_page = *reserve_pages;
		*reserve_pages = Some(first_additional_page);

		page.as_mut().next_page = *bin;
		*bin = Some(page);

		let ret = page.as_mut().alloc(object_size);
		debug_assert!(ret.is_some());
		unsafe { ret.unwrap_unchecked() }.as_ptr()
	} else {
		// OOM?
		ptr::null_mut()
	}
}
