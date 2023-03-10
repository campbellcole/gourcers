{ lib, fetchCrate, rustPlatform, openssl, pkg-config, python3 }:

rustPlatform.buildRustPackage rec {
  pname = "qsv";
  version = "0.91.0";

  src = fetchCrate {
    inherit pname version;
    sha256 = "sha256-QcjQ7CP1fsxPV4Z4oZk8kKk/Wwyc8WSqUPVUou5kWu0=";
  };

  buildFeatures = [ "full" ];

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ openssl ];

  doCheck = false;

  cargoSha256 = "sha256-DavXv4clBzyHgh7UEdnpipzZwP7Mkh/crnOpzuPuv9Y=";
}