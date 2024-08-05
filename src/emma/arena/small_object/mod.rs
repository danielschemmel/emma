use core::mem::{offset_of, MaybeUninit};
use core::num::NonZero;
use core::ptr::NonNull;

use static_assertions::{const_assert, const_assert_eq};
#[cfg(feature = "tls")]
use {
	crate::emma::{AtomicHeapId, HeapId},
	core::sync::atomic::{AtomicU32, Ordering},
};

use crate::mmap::mmap_aligned;

const ARENA_SIZE: u32 = 4 * 1024 * 1024;
const PAGE_SIZE: u32 = 128 * 1024;
const PAGES_PER_ARENA: u32 = ARENA_SIZE / PAGE_SIZE;
const MAXIMUM_OBJECT_ALIGNMENT: u32 = 1024;
#[cfg(not(feature = "tls"))]
const METADATA_ZONE_SIZE: u32 = MAXIMUM_OBJECT_ALIGNMENT;
#[cfg(feature = "tls")]
const METADATA_ZONE_SIZE: u32 = MAXIMUM_OBJECT_ALIGNMENT;

#[derive(Debug)]
pub struct SmallObjectArena {
	#[cfg(feature = "tls")]
	owner: AtomicHeapId,
	pages: [SmallObjectPage; PAGES_PER_ARENA as usize],
}

const_assert!(ARENA_SIZE.is_power_of_two());
const_assert!(PAGE_SIZE.is_power_of_two());
const_assert!(MAXIMUM_OBJECT_ALIGNMENT.is_power_of_two());
const_assert_eq!(ARENA_SIZE % PAGE_SIZE, 0);
const_assert_eq!(PAGE_SIZE % MAXIMUM_OBJECT_ALIGNMENT, 0);
const_assert_eq!(METADATA_ZONE_SIZE % MAXIMUM_OBJECT_ALIGNMENT, 0);
const_assert!(size_of::<SmallObjectArena>() > (METADATA_ZONE_SIZE - MAXIMUM_OBJECT_ALIGNMENT) as usize);
const_assert!(size_of::<SmallObjectArena>() <= METADATA_ZONE_SIZE as usize);

impl SmallObjectArena {
	#[inline]
	unsafe fn arena(p: NonNull<u8>) -> NonNull<SmallObjectArena> {
		NonNull::new_unchecked(((p.as_ptr() as usize) & !(ARENA_SIZE as usize - 1)) as *mut SmallObjectArena)
	}

	#[inline]
	unsafe fn object_offset(p: NonNull<u8>) -> NonZero<u32> {
		NonZero::new_unchecked((p.as_ptr() as u32) % ARENA_SIZE)
	}
}

#[derive(Debug)]
pub struct SmallObjectPage {
	pub next_page: Option<NonNull<SmallObjectPage>>,
	page_number: u32,
	free_list: Option<NonZero<u32>>,
	#[cfg(feature = "tls")]
	foreign_free_list: AtomicU32,
	bytes_in_reserve: u32,
}

impl SmallObjectPage {
	#[inline]
	pub unsafe fn from_new_arena(
		#[cfg(feature = "tls")] owner: HeapId,
	) -> Option<(
		NonNull<SmallObjectPage>,
		NonNull<SmallObjectPage>,
		NonNull<SmallObjectPage>,
	)> {
		let region = mmap_aligned(
			NonZero::new(ARENA_SIZE as usize).unwrap(),
			NonZero::new(ARENA_SIZE as usize).unwrap(),
			3,
		)?;

		let pages_p = region
			.byte_add(offset_of!(SmallObjectArena, pages))
			.cast::<SmallObjectPage>();
		let mut pages: [MaybeUninit<SmallObjectPage>; PAGES_PER_ARENA as usize] = MaybeUninit::uninit().assume_init();
		pages[0].write(SmallObjectPage {
			next_page: None,
			page_number: 0,
			free_list: None,
			#[cfg(feature = "tls")]
			foreign_free_list: AtomicU32::new(0),
			bytes_in_reserve: PAGE_SIZE - METADATA_ZONE_SIZE,
		});
		for i in 1..pages.len() - 1 {
			pages[i].write(SmallObjectPage {
				next_page: Some(pages_p.add(i + 1)),
				page_number: i as u32,
				free_list: None,
				#[cfg(feature = "tls")]
				foreign_free_list: AtomicU32::new(0),
				bytes_in_reserve: PAGE_SIZE,
			});
		}
		pages[pages.len() - 1].write(SmallObjectPage {
			next_page: None,
			page_number: (pages.len() - 1) as u32,
			free_list: None,
			#[cfg(feature = "tls")]
			foreign_free_list: AtomicU32::new(0),
			bytes_in_reserve: PAGE_SIZE,
		});

		region.cast().write(SmallObjectArena {
			#[cfg(feature = "tls")]
			owner: AtomicHeapId::new(owner),
			pages: core::mem::transmute(pages),
		});

		Some((pages_p, pages_p.add(1), pages_p.add(PAGES_PER_ARENA as usize - 1)))
	}

	#[inline]
	unsafe fn page_id(p: *mut u8) -> usize {
		((p as usize) & (ARENA_SIZE as usize - 1)) / (PAGE_SIZE as usize)
	}

	#[inline]
	pub fn alloc(&mut self, object_size: u32) -> Option<NonNull<u8>> {
		if let Some(offset) = self.free_list {
			unsafe {
				let p = SmallObjectArena::arena(NonNull::new_unchecked(self).cast())
					.byte_add(offset.get() as usize)
					.cast();
				self.free_list = p.cast::<Option<NonZero<u32>>>().read();

				Some(p)
			}
		} else {
			#[cfg(feature = "tls")]
			{
				if let Some(offset) = NonZero::new(self.foreign_free_list.swap(0, Ordering::Acquire)) {
					unsafe {
						let p = SmallObjectArena::arena(NonNull::new_unchecked(self).cast())
							.byte_add(offset.get() as usize)
							.cast();
						self.free_list = p.cast::<Option<NonZero<u32>>>().read();

						return Some(p);
					}
				}
			}

			if self.bytes_in_reserve >= object_size {
				unsafe {
					let p = SmallObjectArena::arena(NonNull::new_unchecked(self).cast())
						.cast::<u8>()
						.byte_add(((self.page_number + 1) * PAGE_SIZE - self.bytes_in_reserve) as usize);
					self.bytes_in_reserve -= object_size;

					if self.bytes_in_reserve % 4096 >= object_size {
						let mut q = p.byte_add(object_size as usize);
						let mut offset = SmallObjectArena::object_offset(q);
						self.free_list = Some(offset);
						self.bytes_in_reserve -= object_size;

						while self.bytes_in_reserve % 4096 >= object_size {
							let next = q.byte_add(object_size as usize);
							offset = offset.checked_add(object_size).unwrap_unchecked();
							q.cast::<Option<NonZero<u32>>>().write(Some(offset));
							self.bytes_in_reserve -= object_size;
							q = next;
						}
						q.cast::<Option<NonZero<u32>>>().write(None);
					}

					Some(p)
				}
			} else {
				None
			}
		}
	}

	#[cfg(not(feature = "tls"))]
	#[inline]
	pub unsafe fn dealloc(p: NonNull<u8>) {
		let page = &mut unsafe { SmallObjectArena::arena(p).as_mut() }.pages[SmallObjectPage::page_id(p.as_ptr())];
		p.cast::<Option<NonZero<u32>>>().write(page.free_list);
		page.free_list = Some(SmallObjectArena::object_offset(p));
	}

	#[cfg(feature = "tls")]
	#[inline]
	pub unsafe fn dealloc(heap_id: HeapId, p: NonNull<u8>) {
		let arena = SmallObjectArena::arena(p);
		let mut page = arena
			.byte_add(offset_of!(SmallObjectArena, pages))
			.cast::<SmallObjectPage>()
			.add(SmallObjectPage::page_id(p.as_ptr()));

		let p_offset = SmallObjectArena::object_offset(p);

		let owner = arena
			.byte_add(offset_of!(SmallObjectArena, owner))
			.cast::<AtomicHeapId>()
			.as_ref()
			.load(Ordering::Relaxed);
		if owner == heap_id {
			let page = page.as_mut();
			p.cast::<Option<NonZero<u32>>>().write(page.free_list);
			page.free_list = Some(p_offset);
		} else {
			let free_list = page
				.byte_add(offset_of!(SmallObjectPage, foreign_free_list))
				.cast::<AtomicU32>()
				.as_ref();
			loop {
				let next = free_list.load(Ordering::Relaxed);
				p.cast::<Option<NonZero<u32>>>().write(NonZero::new(next));
				if free_list
					.compare_exchange(next, p_offset.get(), Ordering::Release, Ordering::Relaxed)
					.is_ok()
				{
					break;
				}
			}
		}
	}
}
