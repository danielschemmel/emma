use core::num::NonZero;
use core::ptr::NonNull;

use static_assertions::{const_assert, const_assert_eq};

use crate::mmap::mmap_aligned;

const ARENA_SIZE: u32 = 4 * 1024 * 1024;
const PAGE_SIZE: u32 = 128 * 1024;
const PAGES_PER_ARENA: u32 = ARENA_SIZE / PAGE_SIZE;
const MAXIMUM_OBJECT_ALIGNMENT: u32 = 1024;
const METADATA_ZONE_SIZE: u32 = MAXIMUM_OBJECT_ALIGNMENT;

#[derive(Debug)]
pub struct SmallObjectArena {
	pub pages: [SmallObjectPage; PAGES_PER_ARENA as usize],
}

const_assert!(MAXIMUM_OBJECT_ALIGNMENT.is_power_of_two());
const_assert_eq!(METADATA_ZONE_SIZE % MAXIMUM_OBJECT_ALIGNMENT, 0);
const_assert!(size_of::<SmallObjectArena>() <= MAXIMUM_OBJECT_ALIGNMENT as usize);

impl SmallObjectArena {
	pub unsafe fn new() -> Option<NonNull<SmallObjectArena>> {
		let region = mmap_aligned(
			NonZero::new(ARENA_SIZE as usize).unwrap(),
			NonZero::new(ARENA_SIZE as usize).unwrap(),
			3,
		)?;

		let mut region: NonNull<SmallObjectArena> = region.cast();
		region.write(Self {
			pages: Default::default(),
		});

		let arena = region.as_mut();
		arena.pages[0].page_number = 0;
		arena.pages[0].bytes_in_reserve = PAGE_SIZE - METADATA_ZONE_SIZE;
		for i in 1..arena.pages.len() {
			let page = &mut arena.pages[i];
			page.page_number = i as u32;
			page.bytes_in_reserve = PAGE_SIZE;
		}

		Some(region.cast())
	}

	#[inline]
	fn arena(p: NonNull<u8>) -> NonNull<SmallObjectArena> {
		unsafe { NonNull::new_unchecked(((p.as_ptr() as usize) & !(ARENA_SIZE as usize - 1)) as *mut SmallObjectArena) }
	}
}

#[derive(Debug, Default)]
pub struct SmallObjectPage {
	pub next_page: Option<NonNull<SmallObjectPage>>,
	page_number: u32,
	pub object_size: u32,
	free_list: Option<NonNull<u8>>,
	/// number of objects currently in use
	objects_in_use: u32,
	bytes_in_reserve: u32,
}

impl SmallObjectPage {
	#[inline]
	fn page_id(p: *mut u8) -> usize {
		((p as usize) & (ARENA_SIZE as usize - 1)) / (PAGE_SIZE as usize)
	}

	/// TODO: Measure if passing object size as argument is faster than reading it from the page metadata
	pub fn alloc(&mut self) -> Option<NonNull<u8>> {
		if let Some(p) = self.free_list {
			self.free_list = unsafe { p.cast::<Option<NonNull<u8>>>().read() };

			self.objects_in_use += 1;
			Some(p)
		} else if self.bytes_in_reserve >= self.object_size {
			let p = unsafe {
				SmallObjectArena::arena(NonNull::new_unchecked(self as *mut SmallObjectPage as *mut u8))
					.cast::<u8>()
					.byte_add(((self.page_number + 1) * PAGE_SIZE - self.bytes_in_reserve) as usize)
			};
			self.bytes_in_reserve -= self.object_size;

			self.objects_in_use += 1;
			Some(p)
		} else {
			None
		}
	}

	pub fn dealloc_at(&mut self, p: NonNull<u8>) {
		unsafe { p.cast::<Option<NonNull<u8>>>().write(self.free_list) };
		self.free_list = Some(p);
		self.objects_in_use -= 1;
	}

	pub fn dealloc(p: NonNull<u8>) {
		let page = &mut unsafe { SmallObjectArena::arena(p).as_mut() }.pages[SmallObjectPage::page_id(p.as_ptr())];
		page.dealloc_at(p)
	}
}
