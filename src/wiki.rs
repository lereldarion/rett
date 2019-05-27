use hyper::rt::Future;
use hyper::service::service_fn_ok;
use hyper::{Body, Request, Response, Server};
use tokio::runtime::current_thread;

use std::cell::RefCell;
use std::path::Path;

use horrorshow::{self, Render, RenderOnce, Template};
use relations;

pub fn run(addr: &str, database_file: &Path) {
    let addr = addr.parse().expect("Address::parse");

    let database = ::read_database_from_file(database_file);

    // TODO use hyper send_file example to re-add static files.
    // Routing must be done on req.uri().(method, path).
    // Use a manual small parser lib ?
    // Introduce a ElementDisplayUrl with a parse method ?

    let new_service = || service_fn_ok(|_req| Response::new(Body::from("Blah")));

    let server = Server::bind(&addr)
        .executor(current_thread::TaskExecutor::current())
        .serve(new_service)
        .map_err(|e| panic!("Server error: {}", e));

    current_thread::block_on_all(server).expect("Failed")
}

/* Design:
 *
 * In //:
 * - if stop signal, gracefully shutdown + save db
 * - if request, build page
 *
 * -> need some router-like small tool, see RegexSet
 *
 * Pages:
 * - display for any index
 * - atom creation
 * - abstract creation
 * - link creation:
 *   - buttons to start creating a link from/to a normal display page.
 *   - add get-type params to represent partial state (?link_to=x&...)
 *   - cancel + build button if all requirements are filled
 *
 * Removal TODO
 */

struct State {
    database: RefCell<relations::Database>,
}

trait Page
where
    Self: Sized,
{
    fn to_url(&self) -> String;
    //FIXME fn from_request(request: &Request<()>) -> Option<Self>;
    //FIXME fn generate_page(&self, state: &State) -> Response<()>;
}

struct DisplayElement {
    index: relations::Index,
    // Temporary selection for link creation
    link_from: Option<relations::Index>,
    link_to: Option<relations::Index>,
    link_tag: Option<relations::Index>,
}
impl Page for DisplayElement {
    fn to_url(&self) -> String {
        let query = [
            ("link_from", self.link_from),
            ("link_to", self.link_to),
            ("link_tag", self.link_tag),
        ];
        let mut s = format!("/element/{}", self.index);

        let mut first_query_entry = true;
        for entry in query.into_iter() {
            if let Some(index) = entry.1 {
                let prefix_char = if first_query_entry {
                    first_query_entry = false;
                    '?'
                } else {
                    '&'
                };
                use std::fmt::Write;
                write!(&mut s, "{}{}={}", prefix_char, entry.0, index).unwrap()
            }
        }
        s
    }
}

//TODO use percent-encoding crate for uri handling stuff

mod router {
    // TODO think more about design there
    use hyper::Method;
    use hyper::Uri;

    pub trait FromUri<R> {
        fn from_uri(uri: &Uri) -> Option<R>
        where
            Self: Sized;
    }

    pub struct Router<R> {
        routes: Vec<Box<FromUri<R>>>,
    }

    impl<R> Router<R> {
        pub fn new() -> Self {
            Router { routes: Vec::new() }
        }
    }

    // URLs are percent_encoded.
    // Use simple split on hyper::Uri::path, then use
}
