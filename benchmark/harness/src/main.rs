#[cfg(feature = "emma")]
#[global_allocator]
static GLOBAL: emma::DefaultEmma = emma::DefaultEmma::new();

#[cfg(feature = "std")]
#[allow(dead_code)]
static GLOBAL: () = ();

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(not(any(feature = "emma", feature = "std", feature = "jemalloc", feature = "mimalloc")))]
static_assertions::const_assert!(false, "Please explicitly enable one of the allocator features");

mod arcs;
mod numbers;

fn main() {
	let mut threads = Vec::new();

	threads.push(
		std::thread::Builder::new()
			.name("numbers".to_owned())
			.spawn(numbers::main)
			.unwrap(),
	);
	// threads.push(
	// 	std::thread::Builder::new()
	// 		.name("arcs".to_owned())
	// 		.spawn(arcs::main)
	// 		.unwrap(),
	// );

	for t in threads.into_iter() {
		t.join().unwrap();
	}
}
