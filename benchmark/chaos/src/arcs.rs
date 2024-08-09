use std::sync::Arc;

pub fn main() {
	const WORKERS: usize = 50;
	const ITEMS: usize = 200;

	let (senders, receivers): (Vec<_>, Vec<_>) = (0..WORKERS)
		.map(|_id| std::sync::mpsc::channel::<Arc<Vec<String>>>())
		.unzip();
	let senders = Arc::new(senders);

	let threads: Vec<_> = receivers
		.into_iter()
		.enumerate()
		.map(|(id, receiver)| {
			std::thread::Builder::new()
				.name(format!("arcs/worker/#{id}"))
				.spawn({
					let senders = senders.clone();
					let mut recv_count = 0usize;

					move || {
						for _ in 0..ITEMS {
							let mut values = Vec::new();
							for sender in senders.iter() {
								let mut value = Vec::new();
								for c in '0'..='9' {
									value.push(format!("{c}"));
									value.push(format!("123456789{c}"));
									value.push(format!("12345678901234567890{c}"));
								}
								for c in '0'..='9' {
									value.push(format!("123456789{c}"));
									value.push(format!("12345678901234567890{c}"));
								}
								let value = Arc::new(value);
								sender.send(value.clone()).unwrap();
								values.push(value);
							}
							drop(values);

							while let Ok(_arc) = receiver.try_recv() {
								recv_count += 1;
							}
						}

						while recv_count < WORKERS * ITEMS {
							let _arc = receiver.recv().unwrap();
							recv_count += 1;
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
