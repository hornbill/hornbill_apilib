# Hornbill rust api library

[![hornbill_apilib](https://meritbadge.herokuapp.com/hornbill_apilib)](https://crates.io/crates/hornbill_apilib)

This is an initial commit of the library. It it still a work in progress and some API's might change to make them more effcient.

This library can be used to build tools to communicate with your hornbill instance using the xmlmc endpoint. The documentation for this endpoint can be found [`here`](https://api.hornbill.com/)

## Documentation

[`hornbill_apilib`.](https://docs.rs/hornbill_apilib)

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
hornbill_apilib = "0.1"
```

## Examples

These are examples for using this library:

[`simple`.](https://github.com/hornbill/hornbill_apilib/examples/simple.rs) - quick real world use of the library

[`logon`.](https://github.com/hornbill/hornbill_apilib/examples/logon.rs) - how to logon either with userLogon or setting an apikey.

[`jsonresponse`.](https://github.com/hornbill/hornbill_apilib/examples/jsonresponse.rs) - Requesting a json response back from the server and parsing it using serde_json.

[`responseheaders`.](https://github.com/hornbill/hornbill_apilib/examples/responseheaders.rs) - If you need to see the response headers from api calls.

[`multithreaded`.](https://github.com/hornbill/hornbill_apilib/examples/multithreaded.rs) - WIP, might split this into standard threaded and a tokio example.
