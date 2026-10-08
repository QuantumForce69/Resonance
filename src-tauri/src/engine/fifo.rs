/// Fixed-capacity sample queue that never allocates after creation. When full, the oldest samples are
/// dropped to make room.
pub struct Fifo {
    buf: Vec<f32>,
    head: usize,
    len: usize,
}

impl Fifo {
    pub fn new(capacity: usize) -> Self {
        Self { buf: vec![0.0; capacity.max(1)], head: 0, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    fn push(&mut self, v: f32) {
        let cap = self.buf.len();
        if self.len == cap {
            self.head = (self.head + 1) % cap;
            self.len -= 1;
        }
        self.buf[(self.head + self.len) % cap] = v;
        self.len += 1;
    }

    pub fn push_slice(&mut self, data: &[f32]) {
        for &v in data {
            self.push(v);
        }
    }

    pub fn push_zeros(&mut self, n: usize) {
        for _ in 0..n {
            self.push(0.0);
        }
    }

    /// Drops up to `n` of the oldest samples.
    pub fn discard(&mut self, n: usize) {
        let n = n.min(self.len);
        self.head = (self.head + n) % self.buf.len();
        self.len -= n;
    }

    /// Moves up to `out.len()` of the oldest samples into `out`; returns how many.
    pub fn pop_into(&mut self, out: &mut [f32]) -> usize {
        let n = out.len().min(self.len);
        let cap = self.buf.len();
        for o in out.iter_mut().take(n) {
            *o = self.buf[self.head];
            self.head = (self.head + 1) % cap;
        }
        self.len -= n;
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_order_across_wraparound() {
        let mut f = Fifo::new(4);
        let mut out = [0.0; 4];
        f.push_slice(&[1.0, 2.0, 3.0]);
        assert_eq!(f.pop_into(&mut out[..2]), 2);
        assert_eq!(&out[..2], &[1.0, 2.0]);
        f.push_slice(&[4.0, 5.0, 6.0]);
        assert_eq!(f.len(), 4);
        assert_eq!(f.pop_into(&mut out), 4);
        assert_eq!(out, [3.0, 4.0, 5.0, 6.0]);
        assert!(f.is_empty());
    }

    #[test]
    fn overflow_drops_the_oldest() {
        let mut f = Fifo::new(3);
        f.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mut out = [0.0; 3];
        assert_eq!(f.pop_into(&mut out), 3);
        assert_eq!(out, [3.0, 4.0, 5.0]);
    }

    #[test]
    fn discards_the_oldest() {
        let mut f = Fifo::new(4);
        f.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        f.discard(2);
        let mut out = [0.0; 4];
        assert_eq!(f.pop_into(&mut out), 2);
        assert_eq!(&out[..2], &[4.0, 5.0]);
        f.discard(9);
        assert!(f.is_empty());
    }

    #[test]
    fn pops_only_what_it_has() {
        let mut f = Fifo::new(8);
        f.push_zeros(2);
        f.push_slice(&[7.0]);
        let mut out = [9.0; 5];
        assert_eq!(f.pop_into(&mut out), 3);
        assert_eq!(out, [0.0, 0.0, 7.0, 9.0, 9.0]);
        f.push_slice(&[1.0]);
        f.clear();
        assert_eq!(f.pop_into(&mut out), 0);
    }
}
