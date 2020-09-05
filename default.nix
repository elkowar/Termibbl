with import <nixpkgs> {};
rustPlatform.buildRustPackage rec {
  pname = "termibbl";
  version = "0";

  src = ./.;
  cargoSha256 = "02wgv9493r75bd6bw0lv8pf4hv4k2qdzjw1l7h0rix37avdwfznm";

  nativeBuildInputs = [ pkgconfig ];
  buildInputs = [ openssl.dev ];
}
