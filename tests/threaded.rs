use std::collections::BTreeSet;
use std::sync::Arc;

use emma::DefaultEmma;

#[global_allocator]
static EMMA: DefaultEmma = DefaultEmma::new();

#[test]
fn threaded_arcs() {
	const WORKERS: usize = 50;
	let (senders, receivers): (Vec<_>, Vec<_>) = (0..WORKERS).map(|_id| std::sync::mpsc::channel::<Arc<usize>>()).unzip();
	let senders = Arc::new(senders);

	let threads: Vec<_> = receivers
		.into_iter()
		.enumerate()
		.map(|(id, receiver)| {
			std::thread::Builder::new()
				.name(format!("worker #{id}"))
				.spawn({
					let senders = senders.clone();
					move || {
						let my_id = Arc::new(id);
						for sender in senders.iter() {
							sender.send(my_id.clone()).unwrap();
						}

						let received: BTreeSet<_> = (0..WORKERS).map(|_| receiver.recv().unwrap()).collect();
						assert_eq!(received.first().map(|id| **id), Some(0));
						for (i, j) in received.iter().enumerate() {
							assert_eq!(i, **j);
						}
					}
				})
				.unwrap()
		})
		.collect();

	for handle in threads.into_iter() {
		handle.join().unwrap();
	}
}
