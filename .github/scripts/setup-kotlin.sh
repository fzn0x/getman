#!/usr/bin/env bash
# Installs the Kotlin compiler and the libraries the exported Kotlin snippets
# import (OkHttp and Okio), checking each download against a pinned SHA-256,
# and exports KOTLINC and KOTLIN_CLASSPATH for later steps of a GitHub job.
#
# JetBrains publishes no setup action; the compiler is their release archive.
# Java must already be installed (actions/setup-java).
set -euo pipefail

kotlin=2.4.21
okhttp=4.12.0
okio=3.6.0
dir="${RUNNER_TEMP:-/tmp}/kotlin"
mkdir -p "$dir"

fetch() { # url sha256 file
  curl --fail --silent --show-error --location --output "$dir/$3" "$1"
  echo "$2  $dir/$3" | sha256sum --check --status || {
    echo "::error::$3 does not match its pinned SHA-256"
    exit 1
  }
}

fetch "https://github.com/JetBrains/kotlin/releases/download/v$kotlin/kotlin-compiler-$kotlin.zip" \
  7cc140e76daf416a0424a5557f99afd7ca89ebd463ea1101261e3e01c33e75cd kotlin.zip
fetch "https://repo1.maven.org/maven2/com/squareup/okhttp3/okhttp/$okhttp/okhttp-$okhttp.jar" \
  b1050081b14bb7a3a7e55a4d3ef01b5dcfabc453b4573a4fc019767191d5f4e0 okhttp.jar
fetch "https://repo1.maven.org/maven2/com/squareup/okio/okio-jvm/$okio/okio-jvm-$okio.jar" \
  67543f0736fc422ae927ed0e504b98bc5e269fda0d3500579337cb713da28412 okio.jar

unzip -q "$dir/kotlin.zip" -d "$dir"
{
  echo "KOTLINC=$dir/kotlinc/bin/kotlinc"
  echo "KOTLIN_CLASSPATH=$dir/okhttp.jar:$dir/okio.jar:$dir/kotlinc/lib/kotlin-stdlib.jar"
} >> "${GITHUB_ENV:-/dev/stdout}"
