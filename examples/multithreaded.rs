use hornbill_apilib::*;

fn main() {
    let url = get_url_from_name("demo").expect("We did not get a url for our instance");

    let mut _c = Xmlmc::new(&url).expect("Could not create client");

    //TODO complete a multithreaded example either with a normal thread pool or tokio.
}
