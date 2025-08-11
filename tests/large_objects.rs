use std::alloc::Layout;
use std::ptr::NonNull;

use emma::DefaultEmma;

extern crate alloc;
use alloc::alloc::GlobalAlloc;

static EMMA: DefaultEmma = DefaultEmma::new();

unsafe fn check(objs: &[(NonNull<u8>, Layout)]) {
	let mut sorted = objs.to_owned();
	sorted.sort_by(|a, b| a.0.cmp(&b.0));
	for w in sorted.windows(2) {
		assert_eq!(w.len(), 2);
		let l = w[0];
		let r = w[1];
		assert!(l.0 <= r.0, "sorted");
		assert_ne!(l.0, r.0, "The same object was allocated multiple times!");

		assert!(unsafe { l.0.byte_add(l.1.size()) } <= r.0);
	}

	for &(p, layout) in sorted.iter() {
		let mut i = 0;
		while i + size_of::<usize>() < layout.size() {
			let check = unsafe { p.cast::<usize>().read() };
			assert_eq!(check, p.as_ptr() as usize);
			i += size_of::<usize>();
		}
	}
}

unsafe fn replace_nth(objs: &mut [(NonNull<u8>, Layout)], n: usize, layout: Layout) {
	for (i, o) in objs.iter_mut().enumerate() {
		if i % n == 0 {
			unsafe { EMMA.dealloc(o.0.as_ptr(), o.1) };
			let p = NonNull::new(unsafe { EMMA.alloc(layout) }).unwrap();
			let mut i = 0;
			while i + size_of::<usize>() < layout.size() {
				unsafe { p.cast().write(p.as_ptr() as usize) };
				i += size_of::<usize>();
			}
			*o = (p, layout);
		}
	}
	unsafe { check(objs) };
}

#[test]
fn main() {
	unsafe {
		const COUNT: usize = 10000;
		let mut objs = Vec::with_capacity(COUNT);
		for _ in 0..COUNT {
			let size = 10000;
			let layout = Layout::from_size_align(size, 8).unwrap();
			debug_assert!(layout.size() >= size_of::<usize>());
			debug_assert!(layout.align() >= align_of::<usize>());
			let p = NonNull::new(EMMA.alloc(layout)).unwrap();
			assert_eq!(p.as_ptr() as usize % layout.align(), 0);
			let mut i = 0;
			while i + size_of::<usize>() < layout.size() {
				p.cast().write(p.as_ptr() as usize);
				i += size_of::<usize>();
			}
			objs.push((p, layout));
		}
		check(&objs);

		replace_nth(&mut objs, 2, Layout::from_size_align(12345, 32).unwrap());
		replace_nth(&mut objs, 3, Layout::from_size_align(11111, 16).unwrap());
		replace_nth(&mut objs, 5, Layout::from_size_align(10000, 8).unwrap());

		for o in objs.into_iter() {
			let (p, layout) = o;
			EMMA.dealloc(p.as_ptr(), layout);
		}
	}
}
