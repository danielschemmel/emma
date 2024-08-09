#![feature(new_uninit)]

use core::ffi::c_int;
use core::mem::MaybeUninit;
use core::ptr;
use std::ffi::c_char;
use std::thread;

#[global_allocator]
static ALLOC: ::allocator::Allocator = ::allocator::create_allocator();

struct WorkerArg {
	obj: MaybeUninit<Box<[MaybeUninit<c_char>]>>,
	obj_size: c_int,
	repetitions: c_int,
	iterations: c_int,
}

struct WorkerArgPtr(*mut MaybeUninit<WorkerArg>);

unsafe impl Send for WorkerArgPtr {}

impl WorkerArg {
	pub fn from_args(obj: Box<[MaybeUninit<c_char>]>, obj_size: c_int, repetitions: c_int, iterations: c_int) -> Self {
		Self {
			obj: MaybeUninit::new(obj),
			obj_size,
			repetitions,
			iterations,
		}
	}
}

fn worker(arg: WorkerArgPtr) {
	unsafe {
		let w = arg.0.as_mut().unwrap_unchecked().assume_init_mut();
		drop(w.obj.assume_init_read());
		// the innocuous `workerArg w1 = *w;`:
		let mut w1 = WorkerArg {
			obj: MaybeUninit::uninit(),
			obj_size: w.obj_size,
			repetitions: w.repetitions,
			iterations: w.iterations,
		};
		ptr::copy_nonoverlapping(&w.obj, &mut w1.obj, 1);

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

		let mut w = Box::new_uninit_slice(nthreads.assume_init_read() as usize);
		// While the original code has a constructor for `workerArg`, it just default-initializes all its members, which are
		// all POD types. This means that unless the compiler optimizes the code unusually badly, there is no initialization
		// happening here.

		let mut objs: Box<[MaybeUninit<Box<[MaybeUninit<c_char>]>>]> =
			Box::new_uninit_slice(nthreads.assume_init_read() as usize);
		for i in 0..nthreads.assume_init_read() {
			objs[i as usize].write(Box::new_uninit_slice(obj_size.assume_init_read() as usize));
		}

		let mut t = common::timer::hl::Timer::new();
		t.start();
		for i in 0..nthreads.assume_init_read() {
			w[i as usize].write(WorkerArg::from_args(
				objs[i as usize].assume_init_read(),
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
		drop(objs);
		for w in w.iter_mut() {
			w.assume_init_drop();
		}
		drop(w);

		println!("Time elapsed = {} seconds", t.elapsed_seconds());
	}
}
