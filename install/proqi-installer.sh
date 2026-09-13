#!/bin/sh
set -eu

VERSION='@@VERSION@@'
GLIBC_FLOOR='@@GLIBC_FLOOR@@'
REPOSITORY_URL='https://github.com/oborchers/proqi'
DOWNLOAD_ROOT="$REPOSITORY_URL/releases/download"
MAX_CHECKSUM_BYTES=512
MAX_ARCHIVE_BYTES=134217728

fail() {
  printf 'proqi installer: %s\n' "$*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

request='exact'
destination="${PROQI_INSTALL_DIR:-${HOME:-}/.local/bin}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || fail '--version requires vX.Y.Z or latest'
      request=$2
      shift 2
      ;;
    --prefix)
      [ "$#" -ge 2 ] || fail '--prefix requires an absolute directory below HOME'
      destination=$2
      shift 2
      ;;
    --help|-h)
      printf 'Usage: proqi-installer.sh [--version vX.Y.Z|latest] [--prefix "$HOME/.local/bin"]\n'
      exit 0
      ;;
    *) fail "unknown argument: $1" ;;
  esac
done

need curl
need tar
need uname
need mktemp
need mkdir
need cp
need chmod
need mv
need rm
need cat

case "$request" in
  exact|"$VERSION") ;;
  latest)
    effective=$(curl --fail --silent --show-error --location --proto '=https' \
      --proto-redir '=https' --tlsv1.2 \
      --connect-timeout 10 --max-time 30 --max-redirs 3 --head --output /dev/null \
      --write-out '%{url_effective}' "$REPOSITORY_URL/releases/latest") \
      || fail 'could not resolve the latest stable GitHub Release'
    latest=${effective##*/}
    case "$latest" in v*) ;; *) fail 'latest release returned an invalid stable tag' ;; esac
    latest_numbers=${latest#v}
    case "$latest:$latest_numbers" in
      v*:*[!0-9.]*|v*:.*|v*:*.|v*:*..*) fail 'latest release returned an invalid stable tag' ;;
    esac
    saved_ifs=$IFS
    IFS=.
    set -- $latest_numbers
    IFS=$saved_ifs
    [ "$#" -eq 3 ] || fail 'latest release returned an invalid stable tag'
    for component in "$@"; do
      case "$component" in ''|*[!0-9]*|0[0-9]*) fail 'latest release returned an invalid stable tag' ;; esac
    done
    [ "$latest" = "$VERSION" ] || fail "stale installer is $VERSION, latest stable is $latest"
    ;;
  *) fail "requested version must exactly equal $VERSION" ;;
esac

os=$(uname -s) || fail 'could not determine the operating system'
machine=$(uname -m) || fail 'could not determine the CPU architecture'
case "$machine" in
  x86_64|amd64) cpu='x86_64' ;;
  arm64|aarch64) cpu='aarch64' ;;
  *) fail "unsupported CPU architecture: $machine" ;;
esac

libc='none'
if [ "$os" = 'Linux' ]; then
  libc='ambiguous'
  if command -v getconf >/dev/null 2>&1; then
    glibc=$(getconf GNU_LIBC_VERSION 2>/dev/null || true)
    case "$glibc" in
      'glibc '*)
        glibc=${glibc#glibc }
        major=${glibc%%.*}
        remainder=${glibc#*.}
        minor=${remainder%%.*}
        [ -n "$major" ] && [ -n "$minor" ] || fail "invalid glibc version: $glibc"
        case "$major:$minor" in *[!0-9:]*) fail "invalid glibc version: $glibc" ;; esac
        floor_major=${GLIBC_FLOOR%%.*}
        floor_minor=${GLIBC_FLOOR#*.}
        if [ "$major" -gt "$floor_major" ] || { [ "$major" -eq "$floor_major" ] && [ "$minor" -ge "$floor_minor" ]; }; then
          libc='gnu'
        else
          libc='musl'
        fi
        ;;
    esac
  fi
  if [ "$libc" = 'ambiguous' ] && command -v ldd >/dev/null 2>&1; then
    ldd_report=$(ldd --version 2>&1 || true)
    case "$ldd_report" in *musl*) libc='musl' ;; esac
  fi
  [ "$libc" != 'ambiguous' ] || fail 'could not determine libc from getconf or ldd'
elif [ "$os" != 'Darwin' ]; then
  fail "unsupported operating system: $os"
fi

case "$os:$cpu:$libc" in
@@TARGET_CASES@@
  *) fail "unsupported platform combination: $os $cpu $libc" ;;
esac

[ -n "${HOME:-}" ] || fail 'HOME is required for a user-owned installation'
case "$destination" in /*) ;; *) fail 'installation destination must be absolute' ;; esac
case "$destination" in */../*|*/..|*/./*|*/.) fail 'installation destination must not contain dot path components' ;; esac
home_physical=$(cd "$HOME" 2>/dev/null && pwd -P) || fail 'could not resolve HOME'
[ "$home_physical" != / ] || fail 'HOME must not resolve to the filesystem root'
ancestor=$destination
while [ ! -d "$ancestor" ]; do
  if [ -e "$ancestor" ] || [ -L "$ancestor" ]; then
    fail 'installation destination has a non-directory path component'
  fi
  parent=${ancestor%/*}
  [ -n "$parent" ] || parent=/
  [ "$parent" != "$ancestor" ] || fail 'could not resolve installation destination'
  ancestor=$parent
done
ancestor_physical=$(cd "$ancestor" 2>/dev/null && pwd -P) || fail 'could not resolve installation destination ancestor'
case "$ancestor_physical" in "$home_physical"|"$home_physical"/*) ;; *) fail 'installation destination must remain below HOME' ;; esac
mkdir -p "$destination" || fail "could not create installation destination: $destination"
destination_physical=$(cd "$destination" 2>/dev/null && pwd -P) || fail 'could not resolve installation destination'
case "$destination_physical" in "$home_physical"|"$home_physical"/*) ;; *) fail 'installation destination must remain below HOME' ;; esac
binary_destination="$destination/proqi"
if { [ -e "$binary_destination" ] || [ -L "$binary_destination" ]; } \
  && { [ ! -f "$binary_destination" ] || [ -L "$binary_destination" ]; }; then
  fail 'existing Proqi executable is not a regular file'
fi

temporary=$(mktemp -d "${TMPDIR:-/tmp}/proqi-install.XXXXXX") || fail 'could not create temporary directory'
staging=
cleanup() {
  rm -rf "$temporary"
  [ -z "$staging" ] || rm -rf "$staging"
}
trap cleanup EXIT HUP INT TERM
staging=$(mktemp -d "$destination/.proqi-stage.XXXXXX") || fail 'could not create destination staging directory'
binary_temporary="$staging/proqi"
marker_temporary="$staging/proqi-installation.json"

archive="proqi-$target.tar.gz"
checksum="$archive.sha256"
release_url="$DOWNLOAD_ROOT/$VERSION"
curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
  --connect-timeout 10 --max-time 120 --max-redirs 3 --retry 2 \
  --max-filesize "$MAX_CHECKSUM_BYTES" \
  --output "$temporary/$checksum" "$release_url/$checksum" \
  || fail "could not download checksum for $archive"
curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
  --connect-timeout 10 --max-time 120 --max-redirs 3 --retry 2 \
  --max-filesize "$MAX_ARCHIVE_BYTES" \
  --output "$temporary/$archive" "$release_url/$archive" \
  || fail "could not download $archive"

checksum_record=$(cat "$temporary/$checksum") || fail 'could not read checksum file'
case "$checksum_record" in
  *'
'*) fail 'checksum file must contain one exact record' ;;
esac
set -f
set -- $checksum_record
set +f
[ "$#" -eq 2 ] || fail 'checksum file must contain one exact record'
[ "$2" = "$archive" ] || fail 'checksum names a different archive'
case "$1" in ????????????????????????????????????????????????????????????????) ;; *) fail 'checksum is not SHA-256' ;; esac
case "$1" in *[!0-9a-f]*) fail 'checksum is not lowercase hexadecimal' ;; esac
expected_checksum=$1
if command -v sha256sum >/dev/null 2>&1; then
  checksum_output=$(sha256sum "$temporary/$archive") || fail 'archive checksum verification failed'
elif command -v shasum >/dev/null 2>&1; then
  checksum_output=$(shasum -a 256 "$temporary/$archive") || fail 'archive checksum verification failed'
else
  fail 'sha256sum or shasum is required'
fi
actual_checksum=${checksum_output%% *}
[ "$actual_checksum" = "$expected_checksum" ] || fail 'archive checksum verification failed'

root="proqi-$target"
members="$temporary/members"
tar -tzf "$temporary/$archive" > "$members" || fail 'could not list release archive'
seen_binary=0
seen_license=0
seen_notices=0
seen_marker=0
seen_bash=0
seen_zsh=0
seen_fish=0
while IFS= read -r member; do
  case "$member" in
    "$root/proqi") seen_binary=$((seen_binary + 1)) ;;
    "$root/LICENSE") seen_license=$((seen_license + 1)) ;;
    "$root/THIRD-PARTY-NOTICES.md") seen_notices=$((seen_notices + 1)) ;;
    "$root/proqi-installation.json") seen_marker=$((seen_marker + 1)) ;;
    "$root/completions/proqi.bash") seen_bash=$((seen_bash + 1)) ;;
    "$root/completions/_proqi") seen_zsh=$((seen_zsh + 1)) ;;
    "$root/completions/proqi.fish") seen_fish=$((seen_fish + 1)) ;;
    *) fail "unsafe or unexpected archive member: $member" ;;
  esac
done < "$members"
[ "$seen_binary:$seen_license:$seen_notices:$seen_marker:$seen_bash:$seen_zsh:$seen_fish" = '1:1:1:1:1:1:1' ] \
  || fail 'release archive member set is incomplete or duplicated'

extract="$temporary/extract"
mkdir "$extract" || fail 'could not create extraction directory'
staged="$extract/proqi"
marker="$extract/proqi-installation.json"
tar -xOzf "$temporary/$archive" "$root/proqi" > "$staged" \
  || fail 'could not extract verified release executable'
tar -xOzf "$temporary/$archive" "$root/proqi-installation.json" > "$marker" \
  || fail 'could not extract verified installation marker'
[ -f "$staged" ] && [ ! -L "$staged" ] || fail 'archive executable is not a regular file'
[ -f "$marker" ] && [ ! -L "$marker" ] || fail 'archive installation marker is not a regular file'
chmod 755 "$staged" || fail 'could not make staged executable runnable'
actual=$(PROQI_DISABLE_HERDR=1 "$staged" --version) || fail 'verified executable did not start on this platform'
[ "$actual" = "proqi ${VERSION#v}" ] || fail "verified executable reported unexpected version: $actual"

cp "$staged" "$binary_temporary" || fail 'could not stage executable in destination'
chmod 755 "$binary_temporary" || fail 'could not set executable permissions'
cp "$marker" "$marker_temporary" || fail 'could not stage installation marker'
chmod 644 "$marker_temporary" || fail 'could not set marker permissions'
marker_destination="$destination/proqi-installation.json"
had_marker=0
if [ -f "$marker_destination" ] && [ ! -L "$marker_destination" ]; then
  cp "$marker_destination" "$temporary/previous-marker" \
    || fail 'could not preserve existing installation marker'
  had_marker=1
elif [ -e "$marker_destination" ]; then
  fail 'existing installation marker is not a regular file'
fi
mv "$marker_temporary" "$marker_destination" || fail 'could not install marker'
if ! mv "$binary_temporary" "$binary_destination"; then
  if [ "$had_marker" = 1 ]; then
    cp "$temporary/previous-marker" "$marker_destination" \
      || fail 'could not restore installation marker after executable replacement failed'
  else
    rm -f "$marker_destination"
  fi
  fail 'could not install executable'
fi

printf 'Installed Proqi %s to %s/proqi\n' "$VERSION" "$destination"
case ":${PATH:-}:" in
  *:"$destination":*) ;;
  *) printf 'Add %s to PATH, then run: proqi\n' "$destination" ;;
esac
