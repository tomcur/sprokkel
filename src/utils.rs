use anyhow::anyhow;
use std::{borrow::Cow, path::Path};

/// Turn a path into a URL with a given prefix. If a scheme and host is given, the path becomes an
/// absolute URL.
pub fn path_to_url(
    scheme_and_host: Option<&str>,
    path: impl AsRef<Path>,
) -> anyhow::Result<String> {
    let path = path.as_ref();

    // allocate roughly enough for the resulting string
    let mut builder = String::with_capacity(
        (scheme_and_host.map(|s| s.len() + 1).unwrap_or(0)
            + path.into_iter().map(|p| p.len()).sum::<usize>())
        .next_power_of_two(),
    );
    if let Some(s) = scheme_and_host {
        builder.push_str(s);
    }

    for (idx, part) in path.into_iter().enumerate() {
        if idx > 0 || scheme_and_host.is_some() {
            builder.push('/');
        }
        builder.push_str(part.to_str().ok_or(anyhow!("expected UTF-8 path"))?);
    }

    builder.shrink_to_fit();
    Ok(builder)
}

/// "Map" an iterator returing `Result<T, E>` items by another iterator iterating on `T` items and
/// returning `Result<T, E>` items.
///
/// As long as no error is returned by the inner iterator, calling `next()` on the resulting
/// iterator calls `next()` on the iterator returned by `process`, and its value is returned. If
/// the inner iterator returns an error, the resulting iterator short-circuits, and the output of
/// the call to `next()` will be that error. From that point forward `next()` will always output
/// `None`.
///
/// # Examples
///
/// ```rust
/// let inner_iter = vec![Ok(1), Ok(2), Err(42), Ok(4)].into_iter();
/// let mut resulting_iter =
///     process_results_iter(inner_iter, |iter| iter.map(|val| Ok(val + 100)));
///
/// assert_eq!(resulting_iter.next(), Some(Ok(101)));
/// assert_eq!(resulting_iter.next(), Some(Ok(102)));
/// assert_eq!(resulting_iter.next(), Some(Err(42)));
/// assert_eq!(resulting_iter.next(), None);
/// ```
pub fn process_results_iter<
    'i,
    T: 'i,
    E: 'i,
    I: Iterator<Item = Result<T, E>> + 'i,
    IR: Iterator<Item = Result<T, E>> + 'i,
>(
    iter: I,
    process: impl FnOnce(Box<dyn Iterator<Item = T> + 'i>) -> IR,
) -> impl Iterator<Item = Result<T, E>> + 'i {
    use std::cell::Cell;
    use std::rc::Rc;

    struct InnerIter<E, I> {
        inner_err: Rc<Cell<Option<E>>>,
        iter: I,
    }

    impl<T, E, I> Iterator for InnerIter<E, I>
    where
        I: Iterator<Item = Result<T, E>>,
    {
        type Item = T;

        fn next(&mut self) -> Option<Self::Item> {
            match self.iter.next()? {
                Ok(val) => Some(val),
                Err(err) => {
                    self.inner_err.set(Some(err));
                    None
                }
            }
        }
    }

    struct OuterIter<E, I> {
        inner_err: Rc<Cell<Option<E>>>,
        stop: bool,
        iter: I,
    }

    impl<T, E, I> Iterator for OuterIter<E, I>
    where
        I: Iterator<Item = Result<T, E>>,
    {
        type Item = Result<T, E>;

        fn next(&mut self) -> Option<Self::Item> {
            if self.stop {
                return None;
            }

            // this will likely call out InnerIter::next, which may set inner_err
            let val = self.iter.next();

            if let Some(err) = self.inner_err.take() {
                self.stop = true;
                return Some(Err(err));
            }

            val
        }
    }

    let inner_err: Rc<Cell<Option<E>>> = Rc::new(Cell::new(None));

    let inner_iter = InnerIter {
        inner_err: inner_err.clone(),
        iter,
    };

    OuterIter {
        inner_err,
        stop: false,
        iter: process(Box::new(inner_iter)),
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn path_to_url() {
        use super::path_to_url;
        use std::path::PathBuf;

        assert_eq!(path_to_url(None, "index.html").unwrap(), "index.html");
        assert_eq!(
            path_to_url(Some("https://example.com"), "index.html").unwrap(),
            "https://example.com/index.html"
        );
        assert_eq!(
            path_to_url(
                Some("https://example.com"),
                PathBuf::from("nested").join("file.xml")
            )
            .unwrap(),
            "https://example.com/nested/file.xml"
        );
    }

    #[test]
    pub fn process_results_iter() {
        use super::process_results_iter;

        {
            let inner_iter = vec![Ok(1), Ok(2), Err(42), Ok(4)].into_iter();
            let mut resulting_iter =
                process_results_iter(inner_iter, |iter| iter.map(|val| Ok(val + 100)));

            assert_eq!(resulting_iter.next(), Some(Ok(101)));
            assert_eq!(resulting_iter.next(), Some(Ok(102)));
            assert_eq!(resulting_iter.next(), Some(Err(42)));
            assert_eq!(resulting_iter.next(), None);
        }

        {
            let inner_iter = vec![Ok(1), Ok(2), Ok(3), Ok(4)].into_iter();
            let mut resulting_iter = process_results_iter(inner_iter, |iter| {
                iter.map(|val| if val == 3 { Err(42) } else { Ok(val + 100) })
            });

            assert_eq!(resulting_iter.next(), Some(Ok(101)));
            assert_eq!(resulting_iter.next(), Some(Ok(102)));
            assert_eq!(resulting_iter.next(), Some(Err(42)));
            assert_eq!(resulting_iter.next(), Some(Ok(104)));
        }
    }
}
