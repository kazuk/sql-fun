#[derive(Clone)]
pub(super) struct KeyFragment<'a>(&'a [u8]);

impl<'a> From<&'a [u8]> for KeyFragment<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self(value)
    }
}

impl KeyFragment<'_> {
    pub(super) const fn len(&self) -> usize {
        self.0.len()
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) const fn start_byte(&self) -> u8 {
        self.0[0]
    }

    pub(super) fn common_prefix_len(&self, other: &Self) -> usize {
        let mut common_prefix_len = 0;
        let max_common_prefix = std::cmp::min(self.len(), other.len());
        while common_prefix_len < max_common_prefix
            && self.0[common_prefix_len] == other.0[common_prefix_len]
        {
            common_prefix_len += 1;
        }
        common_prefix_len
    }

    pub(super) fn split_key(&self, other: &Self, common_prefix_len: usize) -> (Self, Self, Self) {
        let common_part = Self::from(&self.0[0..common_prefix_len]);
        let least_key1 = Self::from(&self.0[common_prefix_len..]);
        let least_key2 = Self::from(&other.0[common_prefix_len..]);
        (common_part, least_key1, least_key2)
    }

    pub(super) fn least_key(&self, common_prefix_len: usize) -> Self {
        Self::from(&self.0[common_prefix_len..])
    }

    fn concat_fragments(fragments: &[KeyFragment<'_>]) -> Vec<u8> {
        let mut result_key = Vec::new();
        for fragment in fragments {
            result_key.extend_from_slice(fragment.0);
        }
        result_key
    }

    pub(super) fn to_string_key(
        fragments: &[KeyFragment<'_>],
    ) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(Self::concat_fragments(fragments))
    }

    #[expect(dead_code)]
    #[expect(unsafe_code)]
    unsafe fn to_string_key_unchecked(fragments: &[KeyFragment<'_>]) -> String {
        unsafe { String::from_utf8_unchecked(Self::concat_fragments(fragments)) }
    }
}

// Optimization history (#1121; measured 2026-11-15 with Rust 1.100.0, release):
// Owned byte fragments were tested with SmallVec capacities 8 and 16, including
// capacity 16 with `union`; borrowed KeyFragment remained a slice. Wrapper size
// grew from 16 to 24/32 bytes. In a 577-key rank-payload model, preformatted hits
// slowed from 38.6 ns to 70.7-78.0 ns. Capacity 16 + union reduced construction
// allocations by 46%, but retained map memory grew 9%. Complete-workload medians
// for inline storage alone differed by -1.5% (default) / -0.4% (all), within the
// observed variation, and did not justify adoption. Separately marking heap paths
// with std::hint::cold_path() gave no improvement (+0.22% median, all features).
// These results concern those layouts/hint placements, not all inline storage;
// no custom niche-packed layout was evaluated. Consult the evidence before
// repeating them; assess node layout and lookup costs, not allocation count alone:
// https://github.com/kazuk/sql-fun/pull/1122#issuecomment-6892389106
#[derive(Debug)]
pub(super) struct BoxedKeyFragment(Box<[u8]>);

impl From<KeyFragment<'_>> for BoxedKeyFragment {
    fn from(value: KeyFragment<'_>) -> Self {
        Self(value.0.to_vec().into_boxed_slice())
    }
}

impl BoxedKeyFragment {
    pub(super) fn start_byte(&self) -> u8 {
        self.0[0]
    }

    pub(super) fn as_ref(&self) -> KeyFragment<'_> {
        KeyFragment::from(self.0.as_ref())
    }
}
