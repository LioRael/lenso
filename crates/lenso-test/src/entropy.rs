//! Explicit deterministic entropy for test-support seams only.

use std::{cell::RefCell, rc::Rc};

use sha2::{Digest, Sha256};

/// A reproducible byte stream derived from an explicitly supplied test seed.
///
/// This type is not a cryptographic random-number generator for production
/// work. It never installs a fallback into a Driver, Plugin, or security path;
/// production code must continue to obtain its entropy from its production
/// provider. A test-support seam must opt in by receiving this exact value.
#[derive(Clone, Debug)]
pub struct TestEntropy {
    seed: [u8; 32],
    state: Rc<RefCell<EntropyState>>,
}

#[derive(Debug)]
struct EntropyState {
    next_block: u64,
    block: [u8; 32],
    offset: usize,
}

impl TestEntropy {
    /// Creates a new deterministic stream beginning at the first block.
    pub fn seeded(seed: [u8; 32]) -> Self {
        Self {
            seed,
            state: Rc::new(RefCell::new(EntropyState {
                next_block: 0,
                block: [0; 32],
                offset: 32,
            })),
        }
    }

    /// Fills `output` with the next stable test-only bytes.
    pub fn fill(&self, output: &mut [u8]) {
        let mut copied = 0;
        let mut state = self.state.borrow_mut();
        while copied < output.len() {
            if state.offset == state.block.len() {
                let mut hasher = Sha256::new();
                hasher.update(b"lenso-test-entropy-v1\0");
                hasher.update(self.seed);
                hasher.update(state.next_block.to_be_bytes());
                state.block.copy_from_slice(&hasher.finalize());
                state.next_block = state
                    .next_block
                    .checked_add(1)
                    .expect("a test entropy stream cannot exceed u64 blocks");
                state.offset = 0;
            }
            let remaining = output.len() - copied;
            let available = state.block.len() - state.offset;
            let count = remaining.min(available);
            output[copied..copied + count]
                .copy_from_slice(&state.block[state.offset..state.offset + count]);
            copied += count;
            state.offset += count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_seeds_produce_equal_bytes_across_multiple_blocks() {
        let left = TestEntropy::seeded([7; 32]);
        let right = TestEntropy::seeded([7; 32]);
        let mut left_bytes = [0; 80];
        let mut right_bytes = [0; 80];

        left.fill(&mut left_bytes);
        right.fill(&mut right_bytes);

        assert_eq!(left_bytes, right_bytes);
    }

    #[test]
    fn clones_share_one_explicit_stream() {
        let entropy = TestEntropy::seeded([3; 32]);
        let clone = entropy.clone();
        let baseline = TestEntropy::seeded([3; 32]);
        let mut first = [0; 16];
        let mut second = [0; 16];
        let mut expected = [0; 32];

        entropy.fill(&mut first);
        clone.fill(&mut second);
        baseline.fill(&mut expected);

        assert_eq!([first.as_slice(), second.as_slice()].concat(), expected);
    }
}
