use std::{
    any::Any,
    future::Future,
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
};

pub struct CatchUnwind<F> {
    future: Pin<Box<F>>,
}

impl<F> CatchUnwind<F>
where
    F: Future,
{
    pub fn new(future: F) -> Self {
        Self { future: Box::pin(future) }
    }
}

impl<F> Future for CatchUnwind<F>
where
    F: Future,
{
    type Output = Result<F::Output, Box<dyn Any + Send>>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let self_mut = self.get_mut();
        match catch_unwind(AssertUnwindSafe(|| self_mut.future.as_mut().poll(cx))) {
            Ok(poll) => match poll {
                std::task::Poll::Ready(ok) => std::task::Poll::Ready(Ok(ok)),
                std::task::Poll::Pending => std::task::Poll::Pending,
            },
            Err(err) => std::task::Poll::Ready(Err(err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        futures::{catch_unwind::CatchUnwind, workers::Workers},
        utils,
    };

    #[test]
    fn can_finish_execution() {
        let workers = Workers::new(1);
        let a = utils::poor_mans_random();
        let b = utils::poor_mans_random();
        let f = CatchUnwind::new(async move { a / b });

        let res = workers.queue_with_result(f);

        assert_eq!(a / b, res.unwrap().get().unwrap());
        workers.poison_all();
    }

    #[test]
    fn can_catch_a_panic() {
        let workers = Workers::new(1);
        let f = CatchUnwind::new(async move { panic!("panic") });

        let res = workers.queue_with_result(f).unwrap().get().unwrap_err().downcast::<&str>().unwrap();
        assert_eq!(*res, "panic");
    }
}
