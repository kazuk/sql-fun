#[derive(Clone)]
pub(super) struct KeyFragment<'a>(&'a [u8]);

impl<'a> From<&'a [u8]> for KeyFragment<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self(value)
    }
}

impl KeyFragment<'_> {
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn start_byte(&self) -> u8 {
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

#[derive(Debug)]
pub(super) struct BoxedKeyFragment(Box<[u8]>);

impl<'a> From<KeyFragment<'a>> for BoxedKeyFragment {
    fn from(value: KeyFragment<'_>) -> Self {
        Self(value.0.to_vec().into_boxed_slice())
    }
}

impl BoxedKeyFragment {
    pub(super) fn start_byte(&self) -> u8 {
        self.0[0]
    }

    pub(super) fn as_ref<'a>(&'a self) -> KeyFragment<'a> {
        KeyFragment::from(self.0.as_ref())
    }
}
