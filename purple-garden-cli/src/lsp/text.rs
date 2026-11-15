use std::{collections::HashSet, fmt, sync::Arc};

/// Text the analysis keeps per occurrence, such as a hover or a completion detail. The long part
/// many occurrences repeat, the type of a binding every reference shows, is shared; only the short
/// text around it is each occurrence's own. Joined when a client asks for it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Text {
    before: Box<str>,
    shared: Arc<str>,
    after: Box<str>,
}

impl Text {
    pub(super) fn around(before: impl Into<Box<str>>, shared: Arc<str>, after: impl Into<Box<str>>) -> Self {
        Self {
            before: before.into(),
            shared,
            after: after.into(),
        }
    }

    /// This text between `before` and `after`.
    #[must_use]
    pub(super) fn wrap(&self, before: &str, after: &str) -> Self {
        Self {
            before: format!("{before}{}", self.before).into(),
            shared: Arc::clone(&self.shared),
            after: format!("{}{after}", self.after).into(),
        }
    }

    /// This text sharing its shared part with an equal one in `texts`, or adding it there. A part
    /// already shared, such as a memoized type, stays as is without hashing it again.
    #[must_use]
    pub(super) fn interned(mut self, texts: &mut HashSet<Arc<str>>) -> Self {
        if Arc::strong_count(&self.shared) > 1 {
            return self;
        }
        match texts.get(&self.shared) {
            Some(shared) => self.shared = Arc::clone(shared),
            None => {
                texts.insert(Arc::clone(&self.shared));
            }
        }
        self
    }

    /// This text as a garden code block.
    #[must_use]
    pub(super) fn garden(&self) -> Self {
        self.wrap("```garden\n", "\n```")
    }
}

#[cfg(test)]
impl Text {
    /// Whether both share their shared part.
    pub(super) fn shares(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }
}

impl From<String> for Text {
    fn from(text: String) -> Self {
        Self::around("", text.into(), "")
    }
}

impl From<&str> for Text {
    fn from(text: &str) -> Self {
        Self::around("", text.into(), "")
    }
}

impl From<Arc<str>> for Text {
    fn from(shared: Arc<str>) -> Self {
        Self::around("", shared, "")
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}", self.before, self.shared, self.after)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::Text;

    #[test]
    fn wrapping_shares_the_middle() {
        let shared: Arc<str> = "Record<a: Int>".into();
        let text = Text::around("r: ", Arc::clone(&shared), "").garden();
        assert_eq!(text.to_string(), "```garden\nr: Record<a: Int>\n```");
        assert_eq!(Arc::strong_count(&shared), 2);
    }
}
