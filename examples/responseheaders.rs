use hornbill_apilib::*;

fn main() {
    //We get the url of our instance. We should only ever do this once.
    let url = get_url_from_name("demo").expect("We did not get a url for our instance");

    //We then create our xmlmc object that we can use to query our instance.
    let mut c = Xmlmc::new(&url).expect("Could not create client");

    //We need to tell the xmlmc object to copy headers from any response otherwise it will not do this as it can be ineffcient.
    c.set_copy_headers(true);

    // This requires one input paramets of stage which is an unsignedint
    c.set_param("stage", "1").expect("Could not set stage");

    //We now invoke the call and check the response if its Ok() we are not going to use the string body so we assing it to _ which will throw the result away.
    let _ = match c.invoke("system", "pingCheck") {
        Ok(s) => s,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };

    //Save the headers from the last call to invoke
    let headers = c.get_headers();

    //Print the number of headers present in the headers map
    println!(
        "The number of elements in the headers map: {}",
        headers.len()
    );

    //Loop over all the headers and print their key and value.
    for (key, value) in headers.iter() {
        println!("{:?}: {:?}", key, value);
    }

    //test to see if a single header exists. This is case insensetive
    println!("{}", headers.contains_key("SeRvEr"));
}
