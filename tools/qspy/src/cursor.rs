pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn read_bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        if self.pos + count > self.data.len() {
            None
        } else {
            let slice = &self.data[self.pos..self.pos + count];
            self.pos += count;
            Some(slice)
        }
    }

    pub(crate) fn read_u8(&mut self) -> Option<u8> {
        self.read_bytes(1).map(|b| b[0])
    }

    pub(crate) fn read_u16(&mut self) -> Option<u16> {
        self.read_bytes(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn read_u32(&mut self) -> Option<u32> {
        self.read_bytes(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn read_u64(&mut self) -> Option<u64> {
        self.read_bytes(8)
            .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    pub(crate) fn read_sized(&mut self, size: u8) -> Option<u64> {
        match size {
            1 => self.read_u8().map(u64::from),
            2 => self.read_u16().map(u64::from),
            4 => self.read_u32().map(u64::from),
            8 => self.read_u64(),
            _ => None,
        }
    }

    pub(crate) fn read_c_string(&mut self) -> Option<String> {
        let remaining = &self.data[self.pos..];
        let end = remaining.iter().position(|&b| b == 0)?;
        let bytes = &remaining[..end];
        self.pos += end + 1;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }
}
