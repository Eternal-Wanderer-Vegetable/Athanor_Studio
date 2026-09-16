cd engine
cargo run -p athanor-cli -- import ..\corpus\markdown\basic.md -o demo.azodoc
cargo run -p aludel -- demo.azodoc