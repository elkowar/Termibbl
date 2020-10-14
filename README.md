## Termibbl

A skribbl.io alike, but running in your terminal.

Created in Rust

### Installation

#### Source

#### Nix
```sh
git clone https://github.com/elkowar/Termibbl
cd Termibbl
nix build
```
then
```sh
nix-env -i -f default.nix
```
to run:
```sh
termibbl
```
#### Cargo

```sh
git clone https://github.com/elkowar/Termibbl
cd Termibbl
cargo build --release
```
to run:
```sh
./target/release/termibbl
```
### Creating a server and connecting to it

#### Creating a server
```sh
termibbl server --port <port>
```
##### What port should i use?
If you're uncertain use:
```sh
--port 8888
```
Which should be fine and not conflict with anything.

#### Connecting to a server

```sh
termibbl client --address <public termibbl adress>:<port> <username>
```

### Usage

1. Click on a color to select it

![color](/images/color.gif)

2. Press and hold Left Mouse Button to draw

![draw](/images/draw.gif)

3. Click on the chat to type a message

![chat](/images/chat.gif)

4. Press "delete" to clear your screen

![delete](/images/delete.gif)

5. Press "esc" to quit

![exit](/images/exit.gif)
