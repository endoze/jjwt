{
  description = "jjwt — jujutsu workspace manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    let
      # Overlay that adds `pkgs.jjwt` to nixpkgs.
      overlay = final: prev: {
        jjwt = final.rustPlatform.buildRustPackage {
          pname = "jjwt";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with final; [
            pkg-config
          ];

          buildInputs = with final; [
            openssl
          ] ++ final.lib.optionals final.stdenv.hostPlatform.isDarwin [
            final.darwin.apple_sdk.frameworks.Security
            final.darwin.apple_sdk.frameworks.SystemConfiguration
          ];

          meta = with final.lib; {
            description = "jujutsu-backed workspace manager";
            homepage = "https://github.com/endoze/jjwt";
            license = licenses.mit;
            mainProgram = "jjwt";
          };
        };
      };
    in
    {
      overlays.default = overlay;
    }
    //
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) overlay ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default;
      in
      {
        packages.default = pkgs.jjwt;

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            jujutsu
            cargo-dist
            cargo-tarpaulin
            cargo-binstall
            release-plz
          ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];

          shellHook = ''
            if ! command -v cargo-crap &> /dev/null; then
              echo "Installing cargo-crap via cargo-binstall..."
              cargo binstall cargo-crap --no-confirm 2>/dev/null || cargo install cargo-crap 2>/dev/null || true
            fi
          '';
        };
      }
    );
}
