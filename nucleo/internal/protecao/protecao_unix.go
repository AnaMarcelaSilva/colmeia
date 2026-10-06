//go:build unix

package protecao

import "os"

// Diretorio aplica 0700 à pasta.
func Diretorio(dir string) error { return os.Chmod(dir, 0o700) }

// SoDoUsuario diz se ninguém além do dono tem acesso (grupo e outros sem bits).
func SoDoUsuario(caminho string) (bool, error) {
	info, err := os.Stat(caminho)
	if err != nil {
		return false, err
	}
	return info.Mode().Perm()&0o077 == 0, nil
}
