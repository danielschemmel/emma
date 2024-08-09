#![feature(new_uninit)]

use core::ffi::c_int;
use core::mem::MaybeUninit;
use core::ptr;
use std::thread;

#[global_allocator]
static ALLOC: ::allocator::Allocator = ::allocator::create_allocator();

static mut NITERATIONS: c_int = 50;
static mut NOBJECTS: c_int = 30000;
static mut NTHREADS: c_int = 1;
static mut WORK: c_int = 0;
// never read in the original code
static mut OBJ_SIZE: c_int = 1;

struct Foo {
	_x: c_int,
	_y: c_int,
}

impl Foo {
	pub fn new() -> Self {
		Self { _x: 14, _y: 29 }
	}
}

fn worker() {
	unsafe {
		// `a` is an array of pointers, which are uninitialized in the original.
		let mut a: Box<[MaybeUninit<Box<Foo>>]> = Box::new_uninit_slice((NOBJECTS / NTHREADS) as usize);

		for _j in 0..NITERATIONS {
			for i in 0..NOBJECTS / NTHREADS {
				a[i as usize].write(Box::new(Foo::new()));
				let mut d = 0;
				loop {
					if !(ptr::read_volatile(&d) < WORK) {
						break;
					}

					let mut f = 1;
					let t = ptr::read_volatile(&f) + ptr::read_volatile(&f);
					ptr::write_volatile(&mut f, t);
					let t = ptr::read_volatile(&f) * ptr::read_volatile(&f);
					ptr::write_volatile(&mut f, t);
					let t = ptr::read_volatile(&f) + ptr::read_volatile(&f);
					ptr::write_volatile(&mut f, t);
					let t = ptr::read_volatile(&f) * ptr::read_volatile(&f);
					ptr::write_volatile(&mut f, t);

					let t = ptr::read_volatile(&d);
					ptr::write_volatile(&mut d, t + 1);
				}
				// Here is an assertion in the original, which makes little sense due to `a` being an array of uninitialized
				// pointers.
			}

			for i in 0..NOBJECTS / NTHREADS {
				a[i as usize].assume_init_drop();
				let mut d = 0;
				loop {
					if !(ptr::read_volatile(&d) < WORK) {
						break;
					}

					let mut f = 1;
					let t = ptr::read_volatile(&f) + ptr::read_volatile(&f);
					ptr::write_volatile(&mut f, t);
					let t = ptr::read_volatile(&f) * ptr::read_volatile(&f);
					ptr::write_volatile(&mut f, t);
					let t = ptr::read_volatile(&f) + ptr::read_volatile(&f);
					ptr::write_volatile(&mut f, t);
					let t = ptr::read_volatile(&f) * ptr::read_volatile(&f);
					ptr::write_volatile(&mut f, t);

					let t = ptr::read_volatile(&d);
					ptr::write_volatile(&mut d, t + 1);
				}
			}
		}
	}
}

fn main() {
	unsafe {
		let mut argv = std::env::args().into_iter().fuse();
		let _ = argv.next();
		if let Some(arg) = argv.next() {
			// the original uses atoi without any error checking...
			NTHREADS = arg.parse().unwrap_unchecked();
		}
		if let Some(arg) = argv.next() {
			// the original uses atoi without any error checking...
			NITERATIONS = arg.parse().unwrap_unchecked();
		}
		if let Some(arg) = argv.next() {
			// the original uses atoi without any error checking...
			NOBJECTS = arg.parse().unwrap_unchecked();
		}
		if let Some(arg) = argv.next() {
			// the original uses atoi without any error checking...
			WORK = arg.parse().unwrap_unchecked();
		}
		if let Some(arg) = argv.next() {
			// the original uses atoi without any error checking...
			OBJ_SIZE = arg.parse().unwrap_unchecked();
		}

		// removed the following line from the test, as it really adds nothing except for overhead
		// println!("Running threadtest for {NTHREADS} threads, {NITERATIONS} iterations, {NOBJECTS} objects, {WORK} work and {OBJ_SIZE} objSize...");

		let mut threads: Box<[MaybeUninit<Box<MaybeUninit<thread::JoinHandle<()>>>>]> =
			Box::new_uninit_slice(NTHREADS as usize);

		let start = std::time::Instant::now();

		for i in 0..NTHREADS {
			threads[i as usize].write(Box::new(MaybeUninit::new(
				thread::Builder::new().spawn(worker).unwrap(),
			)));
		}

		for i in 0..NTHREADS {
			threads[i as usize].assume_init_mut().assume_init_read().join().ok();
		}

		let stop = std::time::Instant::now();
		let elapsed = stop - start;

		println!("Time elapsed = {}", elapsed.as_secs_f64());

		// The original code also leaks the objects allocated to hold the threads, while only cleaning up the outer object
		// itself.
		drop(threads);
	}
}
