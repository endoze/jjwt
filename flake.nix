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
          version = "0.1.0";
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
            cargo-tarpaulin
          ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];
        };
      }
    );
}
