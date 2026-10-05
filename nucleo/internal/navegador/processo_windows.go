//go:build windows

package navegador

import (
	"encoding/binary"
	"os"
	"path/filepath"
	"unsafe"

	"golang.org/x/sys/windows"
)

// chromeAberto é o navegador rodando: as pontas do pipe do lado do núcleo,
// esperar (bloqueia até ele sair) e matar (encerra ele e o que ele abriu).
type chromeAberto struct {
	le, escreve *os.File
	esperar     func()
	matar       func()
}

// instalados são os lugares onde o instalador do Chrome e do Edge (que vem
// com o Windows e também aceita o pipe) põem o executável.
func instalados() []string {
	var lista []string
	for _, base := range []string{os.Getenv("ProgramFiles"), os.Getenv("ProgramFiles(x86)"), os.Getenv("LOCALAPPDATA")} {
		if base != "" {
			lista = append(lista, filepath.Join(base, "Google", "Chrome", "Application", "chrome.exe"))
		}
	}
	for _, base := range []string{os.Getenv("ProgramFiles(x86)"), os.Getenv("ProgramFiles")} {
		if base != "" {
			lista = append(lista, filepath.Join(base, "Microsoft", "Edge", "Application", "msedge.exe"))
		}
	}
	return lista
}

// inicioComReserva é o STARTUPINFOEXW com os campos cbReserved2 e
// lpReserved2, que o x/sys esconde: é por eles que a biblioteca C de um
// programa recebe descritores além de 0, 1 e 2.
type inicioComReserva struct {
	Cb            uint32
	_             *uint16
	Desktop       *uint16
	Title         *uint16
	X             uint32
	Y             uint32
	XSize         uint32
	YSize         uint32
	XCountChars   uint32
	YCountChars   uint32
	FillAttribute uint32
	Flags         uint32
	ShowWindow    uint16
	CbReserved2   uint16
	LpReserved2   *byte
	StdInput      windows.Handle
	StdOutput     windows.Handle
	StdErr        windows.Handle
	Atributos     *windows.ProcThreadAttributeList
}

// reservaDescritores monta o bloco que a biblioteca C lê ao iniciar: o
// número de descritores, um byte de flags por descritor e um handle por
// descritor, sem alinhamento. Os descritores 3 e 4 são os pipes; 0 a 2 ficam
// vazios.
func reservaDescritores(fd3, fd4 windows.Handle) []byte {
	const n = 5
	const (
		aberto = 0x01 // FOPEN
		pipe   = 0x08 // FPIPE
	)
	tamanho := int(unsafe.Sizeof(uintptr(0)))
	bloco := make([]byte, 4+n+n*tamanho)
	binary.LittleEndian.PutUint32(bloco, n)
	handles := []windows.Handle{windows.InvalidHandle, windows.InvalidHandle, windows.InvalidHandle, fd3, fd4}
	for i, h := range handles {
		if i >= 3 {
			bloco[4+i] = aberto | pipe
		}
		posicao := 4 + n + i*tamanho
		if tamanho == 8 {
			binary.LittleEndian.PutUint64(bloco[posicao:], uint64(h))
		} else {
			binary.LittleEndian.PutUint32(bloco[posicao:], uint32(h))
		}
	}
	return bloco
}

// abrirChrome inicia o navegador com o pipe de controle nos descritores 3 (o
// Chrome lê os comandos) e 4 (o Chrome escreve as respostas), como no Linux.
// Só essas duas pontas são herdadas, e o navegador fica num job: matar
// encerra ele e os processos que ele abriu.
func abrirChrome(executavel string, args []string) (*chromeAberto, error) {
	herdavel := windows.SecurityAttributes{InheritHandle: 1}
	herdavel.Length = uint32(unsafe.Sizeof(herdavel))
	var chromeLe, nucleoEscreve, nucleoLe, chromeEscreve windows.Handle
	if err := windows.CreatePipe(&chromeLe, &nucleoEscreve, &herdavel, 0); err != nil {
		return nil, err
	}
	if err := windows.CreatePipe(&nucleoLe, &chromeEscreve, &herdavel, 0); err != nil {
		windows.CloseHandle(chromeLe)
		windows.CloseHandle(nucleoEscreve)
		return nil, err
	}
	fecharTudo := func() {
		for _, h := range []windows.Handle{chromeLe, nucleoEscreve, nucleoLe, chromeEscreve} {
			windows.CloseHandle(h)
		}
	}
	// As pontas do núcleo não vão para o navegador.
	for _, h := range []windows.Handle{nucleoEscreve, nucleoLe} {
		if err := windows.SetHandleInformation(h, windows.HANDLE_FLAG_INHERIT, 0); err != nil {
			fecharTudo()
			return nil, err
		}
	}
	atributos, err := windows.NewProcThreadAttributeList(1)
	if err != nil {
		fecharTudo()
		return nil, err
	}
	defer atributos.Delete()
	herdados := []windows.Handle{chromeLe, chromeEscreve}
	if err := atributos.Update(windows.PROC_THREAD_ATTRIBUTE_HANDLE_LIST, unsafe.Pointer(&herdados[0]), uintptr(len(herdados))*unsafe.Sizeof(herdados[0])); err != nil {
		fecharTudo()
		return nil, err
	}
	trabalho, err := windows.CreateJobObject(nil, nil)
	if err != nil {
		fecharTudo()
		return nil, err
	}
	limites := windows.JOBOBJECT_EXTENDED_LIMIT_INFORMATION{}
	limites.BasicLimitInformation.LimitFlags = windows.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
	if _, err := windows.SetInformationJobObject(trabalho, windows.JobObjectExtendedLimitInformation, uintptr(unsafe.Pointer(&limites)), uint32(unsafe.Sizeof(limites))); err != nil {
		windows.CloseHandle(trabalho)
		fecharTudo()
		return nil, err
	}

	reserva := reservaDescritores(chromeLe, chromeEscreve)
	inicio := inicioComReserva{CbReserved2: uint16(len(reserva)), LpReserved2: &reserva[0], Atributos: atributos.List()}
	inicio.Cb = uint32(unsafe.Sizeof(inicio))
	linha, err := windows.UTF16PtrFromString(windows.ComposeCommandLine(append([]string{executavel}, args...)))
	if err != nil {
		windows.CloseHandle(trabalho)
		fecharTudo()
		return nil, err
	}
	var info windows.ProcessInformation
	opcoes := uint32(windows.EXTENDED_STARTUPINFO_PRESENT | windows.CREATE_SUSPENDED)
	err = windows.CreateProcess(nil, linha, nil, nil, true, opcoes, nil, nil, (*windows.StartupInfo)(unsafe.Pointer(&inicio)), &info)
	// As pontas do navegador já foram duplicadas para ele (ou ele não abriu).
	windows.CloseHandle(chromeLe)
	windows.CloseHandle(chromeEscreve)
	if err != nil {
		windows.CloseHandle(trabalho)
		windows.CloseHandle(nucleoLe)
		windows.CloseHandle(nucleoEscreve)
		return nil, err
	}
	if err := windows.AssignProcessToJobObject(trabalho, info.Process); err != nil {
		windows.TerminateProcess(info.Process, 1)
		windows.CloseHandle(info.Thread)
		windows.CloseHandle(info.Process)
		windows.CloseHandle(trabalho)
		windows.CloseHandle(nucleoLe)
		windows.CloseHandle(nucleoEscreve)
		return nil, err
	}
	windows.ResumeThread(info.Thread)
	windows.CloseHandle(info.Thread)
	return &chromeAberto{
		le:      os.NewFile(uintptr(nucleoLe), "navegador-le"),
		escreve: os.NewFile(uintptr(nucleoEscreve), "navegador-escreve"),
		esperar: func() {
			windows.WaitForSingleObject(info.Process, windows.INFINITE)
			windows.CloseHandle(info.Process)
			// Fechar o job encerra o que tenha sobrado do navegador.
			windows.CloseHandle(trabalho)
		},
		matar: func() { windows.TerminateJobObject(trabalho, 1) },
	}, nil
}
