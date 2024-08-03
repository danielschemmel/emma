use std::collections::BTreeMap;

use num_bigint::{BigInt, Sign};

pub fn main() {
	const CHECKSUM: u128 = 204866386859335242080176062813243914293;

	const LIMIT: u64 = 50000;
	const LE_BYTES: [u8; 8] = LIMIT.to_le_bytes();
	const LE_BYTES_4: ([u8; 4], [u8; 4]) = (
		[LE_BYTES[0], LE_BYTES[1], LE_BYTES[2], LE_BYTES[3]],
		[LE_BYTES[4], LE_BYTES[5], LE_BYTES[6], LE_BYTES[7]],
	);
	const LE_U32: [u32; 2] = [u32::from_le_bytes(LE_BYTES_4.0), u32::from_le_bytes(LE_BYTES_4.1)];
	let limit: BigInt = BigInt::new(Sign::Plus, Vec::from_iter(LE_U32));

	let mut map = BTreeMap::new();
	{
		let mut i = BigInt::from(2);
		while i < limit {
			map.insert(i.clone(), i.to_string());
			i += BigInt::from(1);
		}
	}
	let sieve = Sieve::new(LIMIT);
	let map2: BTreeMap<_, _> = map
		.iter()
		.rev()
		.map(|(i, s)| (i.clone(), s.clone()))
		.filter(|(i, _s)| {
			let words = i.to_biguint().unwrap().to_u64_digits();
			assert_eq!(words.len(), 1);
			sieve.is_prime(words[0])
		})
		.collect();

	for key in map2.keys() {
		map.remove(key);
	}

	let mut checksum = 0u128;
	for s in map.values() {
		for b in s.bytes() {
			checksum ^= u128::from(b);
			checksum = checksum.rotate_left(3);
		}
	}

	drop(map);

	for s in map2.values() {
		for b in s.bytes() {
			checksum ^= u128::from(b);
			checksum = checksum.rotate_left(3);
		}
	}

	drop(map2);

	assert_eq!(checksum, CHECKSUM);
}

#[derive(Debug)]
struct Sieve {
	sieve: Vec<bool>,
}

impl Sieve {
	pub fn new(limit: u64) -> Self {
		assert!(limit >= 2);

		let mut sieve = Vec::new();
		sieve.resize(((limit - 3) / 2 + 1).try_into().unwrap(), false);

		let mut n = 3;
		loop {
			let i = (n - 3) / 2;
			let mut j = i + n;
			if j >= sieve.len() {
				break;
			}

			if !sieve[i] {
				while {
					sieve[j] = true;
					j += n;
					j < sieve.len()
				} {}
			}
			n += 2;
		}

		Self { sieve }
	}

	pub fn is_prime(&self, value: u64) -> bool {
		assert!(value >= 2);

		if value % 2 == 0 {
			value == 2
		} else {
			!self.sieve[usize::try_from((value - 3) / 2).unwrap()]
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_small_numbers() {
		let sieve = Sieve::new(10);
		assert!(sieve.is_prime(2));
		assert!(sieve.is_prime(3));
		assert!(!sieve.is_prime(4));
		assert!(sieve.is_prime(5));
		assert!(!sieve.is_prime(6));
		assert!(sieve.is_prime(7));
		assert!(!sieve.is_prime(8));
		assert!(!sieve.is_prime(9));
		assert!(!sieve.is_prime(10));
	}

	#[test]
	fn test_mersenne() {
		let sieve = Sieve::new(1 << 19);
		assert!(sieve.is_prime((1 << 2) - 1));
		assert!(!sieve.is_prime(1 << 2));
		assert!(sieve.is_prime((1 << 3) - 1));
		assert!(!sieve.is_prime(1 << 3));
		assert!(sieve.is_prime((1 << 5) - 1));
		assert!(!sieve.is_prime(1 << 5));
		assert!(sieve.is_prime((1 << 7) - 1));
		assert!(!sieve.is_prime(1 << 7));
		assert!(sieve.is_prime((1 << 13) - 1));
		assert!(!sieve.is_prime(1 << 13));
		assert!(sieve.is_prime((1 << 17) - 1));
		assert!(!sieve.is_prime(1 << 17));
		assert!(sieve.is_prime((1 << 19) - 1));
		assert!(!sieve.is_prime(1 << 19));

		assert!(!sieve.is_prime((1 << 4) - 1));
		assert!(!sieve.is_prime(1 << 4));
		assert!(!sieve.is_prime((1 << 6) - 1));
		assert!(!sieve.is_prime(1 << 6));
		assert!(!sieve.is_prime((1 << 8) - 1));
		assert!(!sieve.is_prime(1 << 8));
		assert!(!sieve.is_prime((1 << 9) - 1));
		assert!(!sieve.is_prime(1 << 9));
		assert!(!sieve.is_prime((1 << 10) - 1));
		assert!(!sieve.is_prime(1 << 10));
		assert!(!sieve.is_prime((1 << 11) - 1));
		assert!(!sieve.is_prime(1 << 11));
		assert!(!sieve.is_prime((1 << 12) - 1));
		assert!(!sieve.is_prime(1 << 12));
		assert!(!sieve.is_prime((1 << 14) - 1));
		assert!(!sieve.is_prime(1 << 14));
		assert!(!sieve.is_prime((1 << 15) - 1));
		assert!(!sieve.is_prime(1 << 15));
		assert!(!sieve.is_prime((1 << 16) - 1));
		assert!(!sieve.is_prime(1 << 16));
		assert!(!sieve.is_prime((1 << 18) - 1));
		assert!(!sieve.is_prime(1 << 18));
	}
}
