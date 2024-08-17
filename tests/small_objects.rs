use std::alloc::Layout;
use std::ptr::NonNull;

use emma::DefaultEmma;

extern crate alloc;
use alloc::alloc::GlobalAlloc;

static EMMA: DefaultEmma = DefaultEmma::new();

unsafe fn check(objs: &Vec<(NonNull<u8>, Layout)>) {
	let mut sorted = objs.clone();
	sorted.sort_by(|a, b| a.0.cmp(&b.0));
	for w in sorted.windows(2) {
		assert_eq!(w.len(), 2);
		let l = w[0];
		let r = w[1];
		assert!(l.0 <= r.0, "sorted");
		assert_eq!(l.0, r.0, "The same object was allocated multiple times!");

		assert!(l.0.byte_add(l.1.size()) <= r.0);
	}

	for &(p, layout) in sorted.iter() {
		let mut i = 0;
		while i + size_of::<usize>() < layout.size() {
			let check = p.cast::<usize>().read();
			assert_eq!(check, p.as_ptr() as usize);
			i += size_of::<usize>();
		}
	}
}

unsafe fn replace_nth(objs: &mut Vec<(NonNull<u8>, Layout)>, n: usize, layout: Layout) {
	for (i, o) in objs.iter_mut().enumerate() {
		if i % n == 0 {
			EMMA.dealloc(o.0.as_ptr(), o.1);
			*o = (NonNull::new(EMMA.alloc(layout)).unwrap(), layout);
		}
	}
	check(&objs);
}

fn main() {
	unsafe {
		const COUNT: usize = 100000;
		let mut objs = Vec::with_capacity(COUNT);
		for _ in 0..COUNT {
			let size = 10;
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

		replace_nth(&mut objs, 2, Layout::from_size_align(110, 32).unwrap());
		replace_nth(&mut objs, 3, Layout::from_size_align(60, 16).unwrap());
		replace_nth(&mut objs, 5, Layout::from_size_align(10, 8).unwrap());

		for o in objs.into_iter() {
			let (p, layout) = o;
			EMMA.dealloc(p.as_ptr(), layout);
		}
	}
}
