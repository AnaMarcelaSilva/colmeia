//go:build unix

package protecao

import "os"

// Diretorio aplica 0700 à pasta.
func Diretorio(dir string) error { return os.Chmod(dir, 0o700) }
