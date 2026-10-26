#!/bin/sh
# Instala gesshtor en Linux o Mac: el ejecutable de la última release de GitHub para tu sistema y tu procesador,
# comprobado con el SHA256SUMS de la release (si no coincide, no se instala).
#
#   curl -fsSL https://raw.githubusercontent.com/alrevuelta/gesshtor/main/install.sh | sh
#
# Dónde: GESSHTOR_DIR (si no, ~/.local/bin). Una versión concreta: GESSHTOR_VERSION=v0.1.0.
# Volver a ejecutarlo actualiza a la última.
set -eu

REPO="alrevuelta/gesshtor"

fallo() {
	echo "gesshtor: $*" >&2
	exit 1
}

# Todo dentro de una función que se llama al final: si la descarga del script se corta, no se ejecuta a medias.
instalar() {
	dir="${GESSHTOR_DIR:-$HOME/.local/bin}"
	version="${GESSHTOR_VERSION:-latest}"

	case "$(uname -s)" in
	Linux) sistema=linux ;;
	Darwin) sistema=macos ;;
	*) fallo "este instalador es para Linux y Mac. En Windows, baja el .zip de https://github.com/$REPO/releases/latest" ;;
	esac
	case "$(uname -m)" in
	x86_64 | amd64) cpu=x86_64 ;;
	arm64 | aarch64) cpu=arm64 ;;
	*) fallo "no hay ejecutable para este procesador ($(uname -m))." ;;
	esac
	# Una terminal que va con Rosetta dice x86_64 en un Mac ARM: mejor el ejecutable de ARM.
	if [ "$sistema" = macos ] && [ "$cpu" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
		cpu=arm64
	fi
	if [ "$sistema" = linux ] && [ "$cpu" = arm64 ]; then
		fallo "aún no hay ejecutable para Linux ARM."
	fi

	fichero="gesshtor-$sistema-$cpu.tar.gz"
	if [ "$version" = latest ]; then
		base="https://github.com/$REPO/releases/latest/download"
	else
		base="https://github.com/$REPO/releases/download/$version"
	fi

	tmp="$(mktemp -d)"
	trap 'rm -rf "$tmp"' EXIT
	echo "Bajando $fichero ($version)…"
	bajar "$base/$fichero" "$tmp/$fichero"
	bajar "$base/SHA256SUMS" "$tmp/SHA256SUMS"

	esperado="$(awk -v f="$fichero" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")"
	[ -n "$esperado" ] || fallo "$fichero no está en el SHA256SUMS de la release."
	if command -v sha256sum >/dev/null 2>&1; then
		real="$(sha256sum "$tmp/$fichero" | awk '{ print $1 }')"
	else
		real="$(shasum -a 256 "$tmp/$fichero" | awk '{ print $1 }')"
	fi
	[ "$real" = "$esperado" ] || fallo "lo bajado no coincide con el SHA256SUMS de la release: no se instala."

	tar -xzf "$tmp/$fichero" -C "$tmp"
	chmod +x "$tmp/gesshtor"
	mkdir -p "$dir"
	mv -f "$tmp/gesshtor" "$dir/gesshtor"
	echo "Instalado en $dir/gesshtor: $("$dir/gesshtor" --version)"
	case ":$PATH:" in
	*":$dir:"*) ;;
	*) echo "Para usarlo escribiendo solo «gesshtor», añade $dir a tu PATH (en ~/.zshrc o ~/.bashrc): export PATH=\"$dir:\$PATH\"" ;;
	esac
}

# Solo por https, y con un error claro si no está.
bajar() {
	curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$1" || fallo "no se ha podido bajar $1 (¿hay una release publicada?)."
}

instalar
