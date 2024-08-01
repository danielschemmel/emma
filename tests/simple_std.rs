use emma::DefaultEmma;

#[global_allocator]
static EMMA: DefaultEmma = DefaultEmma::new();

#[test]
fn simple_vecs() {
	let mut v = vec![1, 2, 3];
	let target = v.capacity() + 20;
	while v.len() < target {
		v.push(42);
	}

	let or = v.iter().fold(0, |acc, x| acc | x);
	assert_eq!(or, 42 | 1 | 2 | 3);
}
