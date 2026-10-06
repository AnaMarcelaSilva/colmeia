//go:build windows

package protecao

import (
	"fmt"
	"unsafe"

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

// SoDoUsuario diz se a lista de acesso só dá acesso ao usuário atual e ao
// sistema (herdadas ou não): nem Administradores, nem Usuários, nem Todos.
// Se não, o erro diz quem mais tem acesso.
func SoDoUsuario(caminho string) (bool, error) {
	sd, err := windows.GetNamedSecurityInfo(caminho, windows.SE_FILE_OBJECT, windows.DACL_SECURITY_INFORMATION)
	if err != nil {
		return false, err
	}
	dacl, _, err := sd.DACL()
	if err != nil {
		return false, err
	}
	if dacl == nil {
		return false, fmt.Errorf("%s sem lista de acesso (aberto para todos)", caminho)
	}
	eu, err := usuarioAtual()
	if err != nil {
		return false, err
	}
	for i := range uint32(dacl.AceCount) {
		var entrada *windows.ACCESS_ALLOWED_ACE
		if err := windows.GetAce(dacl, i, &entrada); err != nil {
			return false, err
		}
		// Só herança para os de dentro: não dá acesso a este caminho.
		if entrada.Header.AceFlags&windows.INHERIT_ONLY_ACE != 0 || entrada.Header.AceType != windows.ACCESS_ALLOWED_ACE_TYPE {
			continue
		}
		sid := (*windows.SID)(unsafe.Pointer(&entrada.SidStart))
		if !sid.Equals(eu) && !sid.IsWellKnown(windows.WinLocalSystemSid) {
			return false, fmt.Errorf("%s também dá acesso a %s", caminho, sid)
		}
	}
	return true, nil
}
