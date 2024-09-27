use std::{future::Future, pin::Pin};

use super::{response::Response, AsyncRequest};

pub struct AsyncHandler {
    pub method: String,
    pub path: String,
    pub func: Box<dyn AsyncHandlerFn + Sync>,
}

impl Eq for AsyncHandler {}

impl PartialEq for AsyncHandler {
    fn eq(&self, other: &Self) -> bool {
        self.method == other.method && self.path == other.path
    }
}

impl std::hash::Hash for AsyncHandler {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.method.hash(state);
        self.path.hash(state);
    }
}

impl AsyncHandler {
    fn new(method: &str, path: &str, func: impl AsyncHandlerFn + Send + Sync + 'static) -> AsyncHandler {
        AsyncHandler {
            method: method.to_string(),
            path: path.to_string(),
            func: Box::new(func),
        }
    }

    pub(crate) fn not_found(method: &str) -> AsyncHandler {
        async fn not_found_fn(req: AsyncRequest) -> Result<Response, String> {
            Ok(Response::create(404, format!("Resource: {req_path} not found.", req_path = req.path)))
        }

        AsyncHandler::new("", &method.to_owned(), not_found_fn)
    }
}

impl<T: Send + Sync, F> AsyncHandlerFn for T
where
    T: Fn(AsyncRequest) -> F,
    F: Future<Output = Result<Response, String>> + 'static + Send,
{
    fn call(&self, args: AsyncRequest) -> Pin<Box<dyn Future<Output = Result<Response, String>> + Send + 'static>> {
        Box::pin(self(args))
    }
}
// type CalcFn = Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = i32>  + Send + 'static >> + Send + Sync + 'static >;
trait AsyncHandlerFn: Send {
    fn call(&self, args: AsyncRequest) -> Pin<Box<dyn Future<Output = Result<Response, String>> + Send + 'static>>;
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use crate::{
        futures::workers::Workers,
        http::{response::Response, AsyncRequest},
    };

    use super::AsyncHandler;

    #[test]
    fn z() {
        async fn foo(x: AsyncRequest) -> Result<Response, String> {
            Ok(Response::create(200, x.path))
        }

        let workers = Workers::new(1);
        let some_path = "some_path";
        let res = workers
            .queue_with_result(async move {
                let async_handler = Arc::new(AsyncHandler::new("some method", "some path", foo));
                async_handler.func.call(AsyncRequest::create(some_path, async_handler.clone(), HashMap::new()).clone()).await
            })
            .unwrap()
            .get();

        assert_eq!(res.unwrap().status_code, 200);
    }
}
