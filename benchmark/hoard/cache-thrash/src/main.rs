#![feature(new_uninit)]

use core::ffi::c_int;
use core::mem::MaybeUninit;
use core::ptr;
use std::ffi::c_char;
use std::thread;

#[global_allocator]
static ALLOC: ::allocator::Allocator = ::allocator::create_allocator();

#[derive(Clone, Copy)]
struct WorkerArg {
	obj_size: c_int,
	repetitions: c_int,
	iterations: c_int,
}

struct WorkerArgPtr(*mut MaybeUninit<WorkerArg>);

unsafe impl Send for WorkerArgPtr {}

impl WorkerArg {
	pub fn from_args(obj_size: c_int, repetitions: c_int, iterations: c_int) -> Self {
		Self {
			obj_size,
			repetitions,
			iterations,
		}
	}
}

fn worker(arg: WorkerArgPtr) {
	unsafe {
		let w = arg.0.as_mut().unwrap_unchecked().assume_init_mut();
		let w1 = *w;

		for _i in 0..w1.iterations {
			let mut obj = Box::new_uninit_slice(w1.obj_size as usize);
			for _j in 0..w1.repetitions {
				for k in 0..w1.obj_size {
					obj[k as usize].write(k as c_char);
					let mut ch = *obj[k as usize].assume_init_ref();
					let t = ptr::read_volatile(&ch);
					ptr::write_volatile(&mut ch, t + 1);
				}
			}
			for c in obj.iter_mut() {
				c.assume_init_drop();
			}
			drop(obj);
		}
	}
}

fn main() {
	unsafe {
		let mut nthreads = MaybeUninit::<c_int>::uninit();
		let mut iterations = MaybeUninit::<c_int>::uninit();
		let mut obj_size = MaybeUninit::<c_int>::uninit();
		let mut repetitions = MaybeUninit::<c_int>::uninit();

		let argv = std::env::args();
		let argc = argv.len();
		let mut arg = argv.into_iter();
		let name = arg.next().unwrap_unchecked();
		if argc > 4 {
			nthreads.write(arg.next().unwrap_unchecked().parse().unwrap_unchecked());
			iterations.write(arg.next().unwrap_unchecked().parse().unwrap_unchecked());
			obj_size.write(arg.next().unwrap_unchecked().parse().unwrap_unchecked());
			repetitions.write(arg.next().unwrap_unchecked().parse().unwrap_unchecked());
		} else {
			eprintln!("Usage: {name} nthreads iterations objSize repetitions");
			std::process::exit(1);
		}

		let mut threads = Vec::new();
		threads.resize_with(nthreads.assume_init_read() as usize, || common::fred::hl::Fred::new());
		common::fred::hl::Fred::set_concurrency(thread::available_parallelism().unwrap().get() as i32);

		let mut t = common::timer::hl::Timer::new();
		t.start();

		let mut w = Box::new_uninit_slice(nthreads.assume_init_read() as usize);
		// While the original code has a constructor for `workerArg`, it just default-initializes all its members, which are
		// all POD types. This means that unless the compiler optimizes the code unusually badly, there is no initialization
		// happening here.

		for i in 0..nthreads.assume_init_read() {
			w[i as usize].write(WorkerArg::from_args(
				obj_size.assume_init_read(),
				repetitions.assume_init_read() / nthreads.assume_init_read(),
				iterations.assume_init_read(),
			));
			threads[i as usize].create(worker, WorkerArgPtr(&mut w[i as usize]));
		}

		for i in 0..nthreads.assume_init_read() {
			threads[i as usize].join();
		}
		t.stop();

		drop(threads);
		for w in w.iter_mut() {
			w.assume_init_drop();
		}
		drop(w);

		println!("Time elapsed = {} seconds", t.elapsed_seconds());
	}
}
