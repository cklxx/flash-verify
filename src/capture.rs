use std::collections::VecDeque;
use std::io::{self, Read};

const DEFAULT_LIMIT: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct CapturedStream {
    pub head: String,
    pub tail: String,
    pub bytes: usize,
    pub truncated: bool,
    pub matches: Vec<String>,
}

pub fn read_bounded_with_needles(
    mut reader: impl Read,
    needles: &[String],
) -> io::Result<CapturedStream> {
    let mut buffer = BoundedBuffer::new(DEFAULT_LIMIT);
    let mut scanner = NeedleScanner::new(needles);
    let mut chunk = [0; 8192];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            let mut stream = buffer.finish();
            stream.matches = scanner.finish();
            return Ok(stream);
        }
        scanner.observe(&chunk[..read]);
        buffer.push(&chunk[..read]);
    }
}

struct BoundedBuffer {
    limit: usize,
    head: Vec<u8>,
    tail: VecDeque<u8>,
    bytes: usize,
}

impl BoundedBuffer {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            head: Vec::new(),
            tail: VecDeque::new(),
            bytes: 0,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        self.bytes += chunk.len();
        for byte in chunk {
            self.push_byte(*byte);
        }
    }

    fn push_byte(&mut self, byte: u8) {
        let head_limit = self.limit / 2;
        if self.head.len() < head_limit {
            self.head.push(byte);
            return;
        }
        self.tail.push_back(byte);
        while self.tail.len() > self.limit - head_limit {
            self.tail.pop_front();
        }
    }

    fn finish(self) -> CapturedStream {
        let truncated = self.bytes > self.limit;
        if !truncated {
            return CapturedStream {
                head: full_output(self.head, self.tail),
                tail: String::new(),
                bytes: self.bytes,
                truncated,
                matches: Vec::new(),
            };
        }
        CapturedStream {
            head: String::from_utf8_lossy(&self.head).into_owned(),
            tail: String::from_utf8_lossy(&self.tail.into_iter().collect::<Vec<_>>()).into_owned(),
            bytes: self.bytes,
            truncated,
            matches: Vec::new(),
        }
    }
}

struct NeedleScanner {
    needles: Vec<Vec<u8>>,
    labels: Vec<String>,
    matched: Vec<bool>,
    carry: Vec<u8>,
    carry_limit: usize,
}

impl NeedleScanner {
    fn new(needles: &[String]) -> Self {
        let needles = needles
            .iter()
            .filter(|needle| !needle.is_empty())
            .map(|needle| needle.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let labels = needles
            .iter()
            .map(|needle| String::from_utf8_lossy(needle).into_owned())
            .collect::<Vec<_>>();
        let carry_limit = needles
            .iter()
            .map(|needle| needle.len().saturating_sub(1))
            .max()
            .unwrap_or(0);
        let matched = vec![false; needles.len()];
        Self {
            needles,
            labels,
            matched,
            carry: Vec::new(),
            carry_limit,
        }
    }

    fn observe(&mut self, chunk: &[u8]) {
        if self.needles.is_empty() {
            return;
        }
        let mut window = Vec::with_capacity(self.carry.len() + chunk.len());
        window.extend_from_slice(&self.carry);
        window.extend_from_slice(chunk);
        for (index, needle) in self.needles.iter().enumerate() {
            if !self.matched[index] && contains_bytes(&window, needle) {
                self.matched[index] = true;
            }
        }
        if self.carry_limit == 0 {
            self.carry.clear();
        } else if window.len() <= self.carry_limit {
            self.carry = window;
        } else {
            self.carry = window[window.len() - self.carry_limit..].to_vec();
        }
    }

    fn finish(self) -> Vec<String> {
        self.labels
            .into_iter()
            .zip(self.matched)
            .filter_map(|(label, matched)| matched.then_some(label))
            .collect()
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn full_output(mut head: Vec<u8>, tail: VecDeque<u8>) -> String {
    head.extend(tail);
    String::from_utf8_lossy(&head).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_output_stays_in_head() {
        let captured = read_bounded_with_needles("abc".as_bytes(), &[]).unwrap();
        assert_eq!(captured.head, "abc");
        assert!(!captured.truncated);
    }

    #[test]
    fn large_output_keeps_head_and_tail() {
        let input = vec![b'x'; DEFAULT_LIMIT + 10];
        let captured = read_bounded_with_needles(input.as_slice(), &[]).unwrap();
        assert!(captured.truncated);
        assert_eq!(captured.bytes, DEFAULT_LIMIT + 10);
        assert!(!captured.head.is_empty());
        assert!(!captured.tail.is_empty());
    }

    #[test]
    fn under_limit_output_is_not_split_away() {
        let input = vec![b'x'; DEFAULT_LIMIT - 1];
        let captured = read_bounded_with_needles(input.as_slice(), &[]).unwrap();
        assert!(!captured.truncated);
        assert_eq!(captured.head.len(), DEFAULT_LIMIT - 1);
        assert!(captured.tail.is_empty());
    }

    #[test]
    fn observes_needle_in_truncated_middle() {
        let mut input = vec![b'x'; DEFAULT_LIMIT / 2];
        input.extend_from_slice(b"panic");
        input.extend(vec![b'y'; DEFAULT_LIMIT]);
        let captured = read_bounded_with_needles(input.as_slice(), &["panic".into()]).unwrap();
        assert!(captured.truncated);
        assert_eq!(captured.matches, vec!["panic"]);
    }
}
