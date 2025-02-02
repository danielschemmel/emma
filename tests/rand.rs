#![feature(btree_cursors)]

use std::alloc::Layout;
use std::collections::BTreeMap;
use std::ptr::NonNull;
use std::sync::Mutex;

use emma::DefaultEmma;
use rand_distr::Uniform;

extern crate alloc;
use alloc::alloc::GlobalAlloc;

use rand::prelude::Distribution;
use rand::{Rng, SeedableRng};
use rand_distr::Exp;

static EMMA: CheckedAllocator<DefaultEmma> = CheckedAllocator::new(DefaultEmma::new());

#[derive(Debug)]
struct CheckedAllocator<A: GlobalAlloc> {
	allocator: A,
	map: Mutex<BTreeMap<usize, Layout>>,
}

impl<A: GlobalAlloc> CheckedAllocator<A> {
	pub const fn new(allocator: A) -> Self {
		Self {
			allocator,
			map: Mutex::new(BTreeMap::new()),
		}
	}

	fn track_alloc(&self, p: *mut u8, layout: Layout) {
		if p.is_null() {
			panic!("Allocator out of memory");
		} else {
			assert_eq!((p as usize) & (layout.align() - 1), 0);

			let mut map = self.map.lock().unwrap();
			let mut c = map.lower_bound_mut(std::ops::Bound::Included(&(p as usize)));
			if let Some(next) = c.peek_next() {
				assert!(*next.0 >= (p as usize) + layout.size());
			}
			c.insert_after(p as usize, layout).unwrap();
		}
	}

	fn track_dealloc(&self, p: *mut u8, layout: Layout) {
		let mut map = self.map.lock().unwrap();
		match map.entry(p as usize) {
			std::collections::btree_map::Entry::Vacant(_) => {
				panic!("Address {p:p} was deallocated without being allocated")
			}
			std::collections::btree_map::Entry::Occupied(entry) => {
				assert_eq!(&layout, entry.get());
				entry.remove();
			}
		}
	}
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for CheckedAllocator<A> {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let p = unsafe { self.allocator.alloc(layout) };
		self.track_alloc(p, layout);
		p
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		self.track_dealloc(ptr, layout);
		unsafe { self.allocator.dealloc(ptr, layout) }
	}

	unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
		let p = unsafe { self.allocator.alloc_zeroed(layout) };
		self.track_alloc(p, layout);

		if !p.is_null() {
			for p in (0..layout.size()).map(|i| unsafe { p.byte_add(i) }) {
				assert_eq!(unsafe { p.cast::<u8>().read() }, 0u8);
			}
		}

		p
	}

	unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
		self.track_dealloc(ptr, layout);
		let p = unsafe { self.allocator.realloc(ptr, layout, new_size) };
		self.track_alloc(p, layout);
		p
	}
}

#[test]
fn main() {
	const ITERATIONS: u64 = 100_000;

	let mut rng = rand_chacha::ChaChaRng::seed_from_u64(u64::from_be_bytes(*b"emmaEMMA"));

	let operation_dist = Uniform::new(0, 100).unwrap();
	let size_dist = Exp::<f64>::new(0.00075).unwrap();

	unsafe {
		let mut objs = Vec::new();
		for _ in 0..ITERATIONS {
			let operation = operation_dist.sample(&mut rng);

			if objs.is_empty() || operation < 40 {
				// alloc
				let size = size_dist.sample(&mut rng).min(10000.).max(1.) as usize;
				let layout = Layout::from_size_align(size, 8).unwrap();
				let p = NonNull::new(EMMA.alloc(layout)).unwrap();
				assert_eq!(p.as_ptr() as usize % layout.align(), 0);
				let mut i = 0;
				while i + size_of::<usize>() < layout.size() {
					p.cast().write(p.as_ptr() as usize);
					i += size_of::<usize>();
				}
				objs.push((p, layout));
			} else if operation < 70 {
				// realloc
			} else {
				// dealloc
				let i = rng.random_range(0..objs.len());
				EMMA.dealloc(objs[i].0.as_ptr(), objs[i].1);
				objs.swap_remove(i);
			}
		}

		for o in objs.into_iter() {
			let (p, layout) = o;
			EMMA.dealloc(p.as_ptr(), layout);
		}
	}
}
