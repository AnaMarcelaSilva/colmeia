//go:build windows

package terminal

import "errors"

// No Windows os terminais vão usar ConPTY (ainda não feito).
func Iniciar(comando []string, env []string, dir string) (Pty, error) {
	return nil, errors.New("terminais no Windows ainda não implementados (ConPTY)")
}
