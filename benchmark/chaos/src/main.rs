#[global_allocator]
static ALLOC: ::allocator::Allocator = ::allocator::create_allocator();

mod arcs;
mod numbers;

fn main() {
	let start = std::time::Instant::now();
	for _i in 0..5 {
		let mut threads = Vec::new();

		threads.push(
			std::thread::Builder::new()
				.name("numbers".to_owned())
				.spawn(numbers::main)
				.unwrap(),
		);
		threads.push(
			std::thread::Builder::new()
				.name("arcs".to_owned())
				.spawn(arcs::main)
				.unwrap(),
		);

		for t in threads.into_iter() {
			t.join().unwrap();
		}
	}
	let stop = std::time::Instant::now();
	let elapsed = stop - start;

	println!("Time elapsed = {}", elapsed.as_secs_f64());
}
