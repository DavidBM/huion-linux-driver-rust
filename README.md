# User space Huion GT 191 v2 drivers for Linux and Wayland in Rust

## Usage

You need to have rust in your system. You can use RustUp for that.

```
cargo build --release
sudo ./target/release/huion-drivers-wayland-rs
```

## Ubuntu 19.10

Seems that the tablet already works out of the box with Ubuntu 19.10

## Other tablets. 

Technically this driver should work for many Huion tablets and other brands as they all resell Digipen tablets. The values for Huion GT 191 v2 are hard-coded, let me know if you want to add support for any other tablet and I can extract the configurations to a file.

Pull requests are welcome.
