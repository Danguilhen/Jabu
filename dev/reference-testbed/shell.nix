{ pkgs ? import <nixpkgs> {} }:
let
  php = pkgs.php84.buildEnv {
    extensions = { all, enabled }: with all; [
      pdo_pgsql
      pgsql
      mbstring
      gd
      imagick
      iconv
      zip
      intl
      curl
      sodium
      bcmath
      pcntl
      posix
      fileinfo
      filter
      opcache
      tokenizer
      openssl
      ctype
      dom
      simplexml
    ];
  };
in
pkgs.mkShell {
  buildInputs = [
    php
    pkgs.php84.packages.composer
    pkgs.postgresql
    pkgs.prosody
    pkgs.nginx
    pkgs.galene
    pkgs.git
    pkgs.curl
    pkgs.jq
    pkgs.openssl
  ];
}
