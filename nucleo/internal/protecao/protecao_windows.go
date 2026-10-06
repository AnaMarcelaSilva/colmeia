//go:build windows

package protecao

import (
	"fmt"
	"regexp"

	"golang.org/x/sys/windows"
)

// Diretorio troca a lista de acesso da pasta por uma protegida (sem herdar a
// da pasta de cima) que só dá acesso ao usuário atual e ao sistema; os
// arquivos e pastas de dentro herdam a mesma lista.
func Diretorio(dir string) error {
	sid, err := usuarioAtual()
	if err != nil {
		return err
	}
	sd, err := windows.SecurityDescriptorFromString(fmt.Sprintf("D:P(A;OICI;FA;;;%s)(A;OICI;FA;;;SY)", sid))
	if err != nil {
		return err
	}
	dacl, _, err := sd.DACL()
	if err != nil {
		return err
	}
	return windows.SetNamedSecurityInfo(dir, windows.SE_FILE_OBJECT,
		windows.DACL_SECURITY_INFORMATION|windows.PROTECTED_DACL_SECURITY_INFORMATION, nil, nil, dacl, nil)
}

// ConferirDono recusa uma pasta de outro usuário. Numa conta de
// administrador, o Windows pode pôr o grupo Administradores como dono.
func ConferirDono(dir string) error {
	sd, err := windows.GetNamedSecurityInfo(dir, windows.SE_FILE_OBJECT, windows.OWNER_SECURITY_INFORMATION)
	if err != nil {
		return err
	}
	dono, _, err := sd.Owner()
	if err != nil {
		return err
	}
	eu, err := usuarioAtual()
	if err != nil {
		return err
	}
	admins, err := windows.CreateWellKnownSid(windows.WinBuiltinAdministratorsSid)
	if err != nil {
		return err
	}
	if !dono.Equals(eu) && !dono.Equals(admins) {
		return fmt.Errorf("%s pertence a outro usuário", dir)
	}
	return nil
}

func usuarioAtual() (*windows.SID, error) {
	u, err := windows.GetCurrentProcessToken().GetTokenUser()
	if err != nil {
		return nil, err
	}
	return u.User.Sid, nil
}

// SoDoUsuario diz se a lista de acesso só tem o usuário atual e o sistema
// (herdadas ou não): nem Administradores, nem Usuários, nem Todos.
func SoDoUsuario(caminho string) (bool, error) {
	sd, err := windows.GetNamedSecurityInfo(caminho, windows.SE_FILE_OBJECT, windows.DACL_SECURITY_INFORMATION)
	if err != nil {
		return false, err
	}
	eu, err := usuarioAtual()
	if err != nil {
		return false, err
	}
	entradas := entradaSDDL.FindAllStringSubmatch(sd.String(), -1)
	if len(entradas) == 0 {
		return false, nil
	}
	for _, e := range entradas {
		if e[1] != "SY" && e[1] != eu.String() {
			return false, nil
		}
	}
	return true, nil
}

// entradaSDDL pega o último campo (quem recebe o acesso) de cada entrada.
var entradaSDDL = regexp.MustCompile(`\([^()]*;([^;()]+)\)`)
