use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub fn main() {
	const WORKERS: usize = 50;
	const ITEMS: usize = 200;

	let (senders, receivers): (Vec<_>, Vec<_>) = (0..WORKERS).map(|_id| std::sync::mpsc::channel::<Arc<usize>>()).unzip();
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
					let mut received = BTreeMap::<Arc<usize>, usize>::new();

					move || {
						for _ in 0..ITEMS {
              let mut ids = Vec::new();
							for sender in senders.iter() {
                let my_id = Arc::new(id);
								sender.send(my_id.clone()).unwrap();
                ids.push(my_id);
							}
							drop(ids);

							while let Ok(arc) = receiver.try_recv() {
								recv_count += 1;
								match received.entry(arc) {
									std::collections::btree_map::Entry::Vacant(entry) => {
										entry.insert(1);
									}
									std::collections::btree_map::Entry::Occupied(mut entry) => *entry.get_mut() += 1,
								}
							}
						}

						while recv_count < WORKERS * ITEMS {
							let arc = receiver.recv().unwrap();
							recv_count += 1;
							match received.entry(arc) {
								std::collections::btree_map::Entry::Vacant(entry) => {
									entry.insert(1);
								}
								std::collections::btree_map::Entry::Occupied(mut entry) => *entry.get_mut() += 1,
							}
						}
						for v in received.values() {
							assert_eq!(*v, ITEMS);
						}

						let received: BTreeSet<_> = received.into_keys().collect();
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
