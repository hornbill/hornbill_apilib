# Hornbill rust api library

[![crates.io](https://img.shields.io/crates/v/hornbill_apilib.svg)](https://crates.io/crates/hornbill_apilib)

This library is still a work in progress and some APIs might change to make
them more efficient.

This library can be used to build tools to communicate with your hornbill
instance using either the JSON API (`JsonMC`, preferred) or the xmlmc endpoint
(`Xmlmc`). The documentation for these endpoints can be found
[`here`](https://docs.hornbill.com/)

## Documentation

[`hornbill_apilib`.](https://docs.rs/hornbill_apilib)

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
hornbill_apilib = "0.5"
```

## Examples

These are examples for using this library:

[`simple`.](https://github.com/hornbill/hornbill_apilib/blob/master/examples/simple.rs) -
quick real world use of the library

[`logon`.](https://github.com/hornbill/hornbill_apilib/blob/master/examples/logon.rs) -
how to logon either with userLogon or setting an apikey.

[`jsoninput`.](https://github.com/hornbill/hornbill_apilib/blob/master/examples/jsoninput.rs) -
Sending a request to the JSON API and handling the response.

[`jsonresponse`.](https://github.com/hornbill/hornbill_apilib/blob/master/examples/jsonresponse.rs) -
Requesting a json response back from the server and parsing it using serde_json.

[`responseheaders`.](https://github.com/hornbill/hornbill_apilib/blob/master/examples/responseheaders.rs) -
If you need to see the response headers from api calls.

[`multithreaded`.](https://github.com/hornbill/hornbill_apilib/blob/master/examples/multithreaded.rs) -
How to execute multiple http requests at the same time. requires tokio.
