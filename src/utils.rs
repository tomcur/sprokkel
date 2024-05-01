use anyhow::anyhow;
use std::path::Path;

pub fn path_to_http_url(path: impl AsRef<Path>) -> anyhow::Result<String> {
    let path = path.as_ref();
    let mut builder = String::new();

    for (idx, part) in path.into_iter().enumerate() {
        if idx > 0 {
            builder.push('/');
        }
        builder.push_str(part.to_str().ok_or(anyhow!("expected UTF-8 path"))?);
    }

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
