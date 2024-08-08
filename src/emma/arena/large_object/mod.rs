use core::mem::offset_of;
use core::num::NonZero;
use core::ptr::{self, NonNull};

use static_assertions::const_assert;
#[cfg(feature = "tls")]
use {
	crate::emma::{AtomicHeapId, HeapId},
	core::sync::atomic::{AtomicU32, Ordering},
};

use crate::mmap::alloc_aligned;

const ARENA_SIZE: u32 = 4 * 1024 * 1024;
pub const MAXIMUM_OBJECT_ALIGNMENT: u32 = 512 * 1024;

#[derive(Debug)]
struct Arena {
	#[cfg(feature = "tls")]
	owner: AtomicHeapId,
	page: Page,
}

const_assert!(ARENA_SIZE.is_power_of_two());
const_assert!(MAXIMUM_OBJECT_ALIGNMENT.is_power_of_two());
const_assert!(size_of::<Arena>() < ARENA_SIZE as usize);

impl Arena {
	#[inline]
	unsafe fn arena(p: NonNull<u8>) -> NonNull<Arena> {
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
	free_list: Option<NonZero<u32>>,
	#[cfg(feature = "tls")]
	foreign_free_list: AtomicU32,
	bytes_in_reserve: u32,
}

impl Page {
	#[inline]
	pub unsafe fn from_new_arena(#[cfg(feature = "tls")] owner: HeapId) -> Option<NonNull<Page>> {
		let region = alloc_aligned(
			NonZero::new(ARENA_SIZE as usize).unwrap(),
			NonZero::new(ARENA_SIZE as usize).unwrap(),
			3,
		)?;

		region.cast().write(Arena {
			#[cfg(feature = "tls")]
			owner: AtomicHeapId::new(owner),
			page: Page {
				next_page: None,
				free_list: None,
				#[cfg(feature = "tls")]
				foreign_free_list: AtomicU32::new(0),
				bytes_in_reserve: ARENA_SIZE - size_of::<Arena>() as u32,
			},
		});

		Some(region.byte_add(offset_of!(Arena, page)).cast())
	}

	#[inline]
	pub fn alloc(&mut self, object_size: u32) -> Option<NonNull<u8>> {
		if let Some(offset) = self.free_list {
			unsafe {
				let p = Arena::arena(NonNull::new_unchecked(self).cast())
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
						let p = Arena::arena(NonNull::new_unchecked(self).cast())
							.byte_add(offset.get() as usize)
							.cast();
						self.free_list = p.cast::<Option<NonZero<u32>>>().read();

						return Some(p);
					}
				}
			}

			if self.bytes_in_reserve >= object_size {
				self.bytes_in_reserve -= self.bytes_in_reserve % object_size;
				unsafe {
					let p = Arena::arena(NonNull::new_unchecked(self).cast())
						.cast::<u8>()
						.byte_add((ARENA_SIZE - self.bytes_in_reserve) as usize);
					self.bytes_in_reserve -= object_size;

					return Some(p);
				}
			}

			None
		}
	}

	#[cfg(not(feature = "tls"))]
	#[inline]
	pub unsafe fn dealloc(p: NonNull<u8>) {
		let page = &mut unsafe { Arena::arena(p).as_mut() }.page;
		p.cast::<Option<NonZero<u32>>>().write(page.free_list);
		page.free_list = Some(Arena::object_offset(p));
	}

	#[cfg(feature = "tls")]
	#[inline]
	pub unsafe fn dealloc(heap_id: HeapId, p: NonNull<u8>) {
		let arena = Arena::arena(p);
		let mut page = arena.byte_add(offset_of!(Arena, page)).cast::<Page>();

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

#[inline]
pub unsafe fn alloc(bin: &mut Option<NonNull<Page>>, object_size: u32, #[cfg(feature = "tls")] id: HeapId) -> *mut u8 {
	{
		let mut pp: *mut Option<NonNull<Page>> = bin;
		let mut p = *bin;
		loop {
			if let Some(mut q) = p {
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
			} else {
				break;
			}
		}
	}

	#[cfg(not(feature = "tls"))]
	let page_from_new_arena = Page::from_new_arena();
	#[cfg(feature = "tls")]
	let page_from_new_arena = Page::from_new_arena(id);
	if let Some(mut page) = page_from_new_arena {
		page.as_mut().next_page = *bin;
		*bin = Some(page);

		let ret = page.as_mut().alloc(object_size);
		debug_assert!(ret.is_some());
		return unsafe { ret.unwrap_unchecked() }.as_ptr();
	} else {
		// OOM?
		ptr::null_mut()
	}
}
