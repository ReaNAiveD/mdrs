# MDRS

**MDanceIO Render Service 🎉🎉**

`MDRS` provides remote rendering service based on [MDanceIO](https://github.com/ReaNAiveD/mdanceio) through WebRTC. 

## Get Started

### Prerequisite

`MDRS` is based on [libvpx](https://en.wikipedia.org/wiki/Libvpx) to encode dataframes. 

### Build

The following environment variables should be set. 

```
VPX_STATIC = 1
VPX_VERSION = 1.8.2
VPX_LIB_DIR = <PATH TO LIBVPX>\\lib\\x64
VPX_INCLUDE_DIR = <PATH TO LIBVPX>\\include
```

```bash
cargo build --release
```

### Run from source

This is just a demo to verify that `mdanceio` can perform remote rendering via WebRTC. To execute, you should follow the steps below. 

#### Open the example webpage

[jsfiddle.net](https://jsfiddle.net/r3d974n5/)Visit the jsfiddle and you should see two text-areas and a Button. Wait a minute and copy the text in `Browser base64 Session Description` area. 

> The example page is forked from [webrtc-rs/example](https://github.com/webrtc-rs/webrtc/tree/master/examples/examples/play-from-disk-vpx) directly. I will replace it with GitHub Pages in the near future. 

#### Run the Rendering Service

Paste the text copied into a file named `session_desc.txt` in `private_data` related to your workspace or cwd. 

Then, run in your workspace. 

```bash
cargo run --package mdrs --bin mdrs -- --model <Model Path> --motion <Motion Path>
```

> You can learn how to get valid models and motions in [MDanceIO project README](https://github.com/ReaNAiveD/mdanceio). 

#### Start WebRTC session

Copy the text that `mdrs` just emitted and copy it into `Golang base64 Session Description` area. 

Hit `Start Session` button and enjoy you MMD. 
