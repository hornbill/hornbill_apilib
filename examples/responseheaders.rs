use hornbill_apilib::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //We get the url of our instance. We should only ever do this once.
    let url = get_url_from_name("demo").await?;

    //We then create our xmlmc object that we can use to query our instance.
    let mut c = Xmlmc::new(&url)?;

    //We need to tell the xmlmc object to copy headers from any response otherwise it will not do this as it can be inefficient.
    c.set_copy_headers(true);

    // This requires one input parameter of stage which is an unsigned int
    c.set_param("stage", "1")?;

    //We now invoke the call. We are not going to use the string body so we throw the result away.
    c.invoke("system", "pingCheck").await?;

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

    //test to see if a single header exists. This is case insensitive
    println!("{}", headers.contains_key("SeRvEr"));

    Ok(())
}
