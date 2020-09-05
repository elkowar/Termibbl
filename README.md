## Termibbl

A skribbl.io alike, but running in your terminal.

### Example

![Termibbl](/images/term.gif)

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

### Installation

### Source

#### Nix
```
git clone https://github.com/elkowar/Termibbl
cd Termibbl
nix build
```
then
```
nix-env -i -f default.nix
```
to run:
```
termibbl
```
#### Cargo

```
git clone https://github.com/elkowar/Termibbl
cd Termibbl
cargo build --release
```
to run:
```
./target/release/termibbl
```

