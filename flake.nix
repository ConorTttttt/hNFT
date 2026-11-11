{
  description = "P2P Art Marketplace — Holochain NFT hApp";

  inputs = {
    # Pin the versioned branch, not `main`. Holonix `main` tracks the current
    # dev line (0.8 as of August 2026), so `main` silently moves you off 0.7.
    holonix.url = "github:holochain/holonix?ref=main-0.7";
    # Keep the conductor/CLI on the approved 0.7.1 patch even when
    # holonix/main-0.7 eventually advances to another patch release.
    holonix.inputs.holochain.url = "github:holochain/holochain?ref=holochain-0.7.1";
    holonix.inputs.holochain.flake = false;

    nixpkgs.follows = "holonix/nixpkgs";
    flake-parts.follows = "holonix/flake-parts";
  };

  # Use the public Holochain CI binary cache recommended by Holonix.
  # This avoids building the full conductor from source on clean machines.
  # Only trusted cache signatures are accepted.
  nixConfig = {
    substituters = [
      "https://cache.nixos.org"
      "https://holochain-ci.cachix.org"
    ];
    trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
      "holochain-ci.cachix.org-1:5IUSkZc0aoRS53rfkvH9Kid40NpyjwCMCzwRTXy+QN8="
    ];
  };

  outputs = inputs@{ flake-parts, ... }: flake-parts.lib.mkFlake { inherit inputs; } {
    systems = builtins.attrNames inputs.holonix.devShells;
    perSystem = { inputs', pkgs, ... }: {
      formatter = pkgs.nixpkgs-fmt;

      devShells.default = pkgs.mkShell {
        inputsFrom = [ inputs'.holonix.devShells.default ];

        packages = (with pkgs; [
          nodejs_24
          binaryen
          # One of the holochain crate's build dependencies needs perl on PATH.
          # Only required if you build Sweettest suites; harmless otherwise.
          perl
        ]);

        shellHook = ''
          export PS1='\[\033[1;34m\][holonix:\w]\$\[\033[0m\] '
        '';
      };
    };
  };
}
