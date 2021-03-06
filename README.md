# yeelib_rs

A Rust library for easy interfacing with Yeelight products, including LAN light discovery with [multicast](https://en.wikipedia.org/wiki/Multicast),
with intended implementation of all major parts of the [Yeelight Third-party Control Protocol](https://www.yeelight.com/en_US/developer). 

## Getting started

Unless otherwise specified, methods to adjust the light's parameters have the method name and behavior 
exactly as specified in the above spec.

```rust
use std::time::Duration;
use std::thread::sleep;

use yeelib_rs::YeeClient;
use yeelib_rs::light::Light;
use yeelib_rs::err::YeeError;
use yeelib_rs::req::Transition;
use yeelib_rs::fields::PowerStatus;

fn main() -> Result<(), YeeError> {
    let client = YeeClient::new()?;
    let mut lights: Vec<Light> = client.get_response(Duration::from_secs(1));
    
    for light in lights.iter_mut() {
        light.set_power(PowerStatus::On, Transition::sudden())?;
        sleep(Duration::from_secs(1));
        
        light.set_bright(50, Transition::sudden())?;
        sleep(Duration::from_secs(1));
        
        light.set_ct_abx(3500, 
                         Transition::smooth(Duration::from_millis(400))
                             .unwrap())?;
        sleep(Duration::from_secs(2));
        
        light.toggle()?;
    }
}

```
See [main.rs](src/bin/main.rs) for some more examples.

## Currently supported methods

```
set_ct_abx
set_rgb
set_hsv
set_bright
set_power
toggle
```

## To do

- Document every component
- Flatten public exports
- Finish implementation of the API
- Improve test coverage

## License

Licensed under either of

* Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.