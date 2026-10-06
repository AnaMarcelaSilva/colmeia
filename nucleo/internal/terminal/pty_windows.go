//go:build windows

package terminal

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"
	"unicode/utf16"
	"unsafe"

	"golang.org/x/sys/windows"
)

// Quanto esperar o programa terminar sozinho antes de forçar.
const esperaAoFechar = 3 * time.Second

// ptyWindows é um pseudoconsole (ConPTY): o programa enxerga um console de
// verdade, e o núcleo lê e escreve VT pelos dois pipes, como no Unix.
type ptyWindows struct {
	console  windows.Handle // HPCON
	entrada  *os.File       // o núcleo escreve a digitação
	saida    *os.File       // o núcleo lê o que o programa desenha
	processo windows.Handle
	// trabalho junta o programa e tudo o que ele abrir: fechar o terminal
	// encerra a árvore inteira, como o grupo de processos no Unix.
	trabalho windows.Handle
	fim      chan struct{}
	final    *Saida
	soltar   sync.Once
}

func (p *ptyWindows) Read(b []byte) (int, error)  { return p.saida.Read(b) }
func (p *ptyWindows) Write(b []byte) (int, error) { return p.entrada.Write(b) }

func (p *ptyWindows) Esperar() Saida {
	<-p.fim
	return *p.final
}

func (p *ptyWindows) Redimensionar(colunas, linhas uint16) error {
	return windows.ResizePseudoConsole(p.console, windows.Coord{X: int16(colunas), Y: int16(linhas)})
}

// soltarConsole fecha o pseudoconsole: o programa recebe o aviso de que o
// console fechou e a leitura da saída termina (EOF) depois de esvaziar.
func (p *ptyWindows) soltarConsole() {
	p.soltar.Do(func() { windows.ClosePseudoConsole(p.console) })
}

// Close avisa o programa (o console fecha, como ao fechar a janela) e dá a
// ele a chance de salvar. Se não terminar a tempo, a árvore inteira é
// encerrada pelo job.
func (p *ptyWindows) Close() error {
	go p.soltarConsole()
	select {
	case <-p.fim:
	case <-time.After(esperaAoFechar):
		windows.TerminateJobObject(p.trabalho, 1)
		<-p.fim
	}
	p.entrada.Close()
	return nil
}

// Iniciar abre um pseudoconsole rodando `comando` na pasta `dir`, com as
// variáveis `env` somadas às do núcleo, já no tamanho da tela.
func Iniciar(comando []string, env []string, dir string, tamanho Tamanho) (Pty, error) {
	linha, err := linhaDeComando(comando)
	if err != nil {
		return nil, err
	}
	bloco, err := blocoDeAmbiente(append(append(AmbienteLimpo(os.Environ()), "TERM=xterm-256color", "COLORTERM=truecolor"), env...))
	if err != nil {
		return nil, err
	}
	// Pipes sem herança: só o pseudoconsole usa as pontas de dentro.
	var consoleLe, nucleoEscreve, nucleoLe, consoleEscreve windows.Handle
	if err := windows.CreatePipe(&consoleLe, &nucleoEscreve, nil, 0); err != nil {
		return nil, err
	}
	if err := windows.CreatePipe(&nucleoLe, &consoleEscreve, nil, 0); err != nil {
		windows.CloseHandle(consoleLe)
		windows.CloseHandle(nucleoEscreve)
		return nil, err
	}
	fecharTudo := func() {
		for _, h := range []windows.Handle{consoleLe, nucleoEscreve, nucleoLe, consoleEscreve} {
			windows.CloseHandle(h)
		}
	}
	var console windows.Handle
	tam := windows.Coord{X: int16(tamanho.Colunas), Y: int16(tamanho.Linhas)}
	if err := windows.CreatePseudoConsole(tam, consoleLe, consoleEscreve, 0, &console); err != nil {
		fecharTudo()
		return nil, fmt.Errorf("criando o pseudoconsole: %w", err)
	}
	// O pseudoconsole duplicou as pontas de dentro.
	windows.CloseHandle(consoleLe)
	windows.CloseHandle(consoleEscreve)

	atributos, err := windows.NewProcThreadAttributeList(1)
	if err != nil {
		windows.ClosePseudoConsole(console)
		windows.CloseHandle(nucleoLe)
		windows.CloseHandle(nucleoEscreve)
		return nil, err
	}
	defer atributos.Delete()
	// O valor do atributo é o próprio HPCON (não um ponteiro para ele).
	if err := atributos.Update(windows.PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, *(*unsafe.Pointer)(unsafe.Pointer(&console)), unsafe.Sizeof(console)); err != nil {
		windows.ClosePseudoConsole(console)
		windows.CloseHandle(nucleoLe)
		windows.CloseHandle(nucleoEscreve)
		return nil, err
	}
	processo, trabalho, err := criarProcesso(linha, bloco, dir, atributos)
	if err != nil {
		windows.ClosePseudoConsole(console)
		windows.CloseHandle(nucleoLe)
		windows.CloseHandle(nucleoEscreve)
		return nil, err
	}
	p := &ptyWindows{
		console:  console,
		entrada:  os.NewFile(uintptr(nucleoEscreve), "conpty-entrada"),
		saida:    os.NewFile(uintptr(nucleoLe), "conpty-saida"),
		processo: processo,
		trabalho: trabalho,
		fim:      make(chan struct{}),
		final:    &Saida{},
	}
	go func() {
		windows.WaitForSingleObject(processo, windows.INFINITE)
		var codigo uint32
		windows.GetExitCodeProcess(processo, &codigo)
		p.final.Codigo = int(int32(codigo))
		windows.CloseHandle(processo)
		close(p.fim)
		// Sem o programa, o console fecha: a leitura recebe o resto e o EOF.
		p.soltarConsole()
		// Fechar o job encerra o que tenha sobrado da árvore.
		windows.CloseHandle(trabalho)
	}()
	return p, nil
}

// criarProcesso inicia o programa suspenso, põe no job (com tudo o que ele
// abrir) e solta. Nenhum handle do núcleo é herdado.
func criarProcesso(linha string, ambiente []uint16, dir string, atributos *windows.ProcThreadAttributeListContainer) (processo, trabalho windows.Handle, err error) {
	trabalho, err = windows.CreateJobObject(nil, nil)
	if err != nil {
		return 0, 0, err
	}
	limites := windows.JOBOBJECT_EXTENDED_LIMIT_INFORMATION{}
	limites.BasicLimitInformation.LimitFlags = windows.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
	if _, err = windows.SetInformationJobObject(trabalho, windows.JobObjectExtendedLimitInformation, uintptr(unsafe.Pointer(&limites)), uint32(unsafe.Sizeof(limites))); err != nil {
		windows.CloseHandle(trabalho)
		return 0, 0, err
	}
	linha16, err := windows.UTF16PtrFromString(linha)
	if err != nil {
		windows.CloseHandle(trabalho)
		return 0, 0, err
	}
	dir16, err := windows.UTF16PtrFromString(dir)
	if err != nil {
		windows.CloseHandle(trabalho)
		return 0, 0, err
	}
	var inicio windows.StartupInfoEx
	inicio.Cb = uint32(unsafe.Sizeof(inicio))
	inicio.ProcThreadAttributeList = atributos.List()
	// Entrada e saída padrão vazias de propósito: sem isso o programa herda
	// as do núcleo (redirecionadas para NUL) e escreve nelas, não no console.
	inicio.Flags = windows.STARTF_USESTDHANDLES
	var info windows.ProcessInformation
	opcoes := uint32(windows.EXTENDED_STARTUPINFO_PRESENT | windows.CREATE_UNICODE_ENVIRONMENT | windows.CREATE_SUSPENDED)
	err = windows.CreateProcess(nil, linha16, nil, nil, false, opcoes, &ambiente[0], dir16, &inicio.StartupInfo, &info)
	if err != nil {
		windows.CloseHandle(trabalho)
		return 0, 0, fmt.Errorf("iniciando %s: %w", linha, err)
	}
	if err = windows.AssignProcessToJobObject(trabalho, info.Process); err != nil {
		windows.TerminateProcess(info.Process, 1)
		windows.CloseHandle(info.Thread)
		windows.CloseHandle(info.Process)
		windows.CloseHandle(trabalho)
		return 0, 0, err
	}
	windows.ResumeThread(info.Thread)
	windows.CloseHandle(info.Thread)
	return info.Process, trabalho, nil
}

// linhaDeComando monta a linha do CreateProcess. Um .cmd ou .bat (o claude
// instalado pelo npm, por exemplo) só roda pelo cmd.exe; aí nenhum argumento
// pode ter caractere especial do cmd, que mudaria o que roda.
func linhaDeComando(comando []string) (string, error) {
	if len(comando) == 0 {
		return "", errors.New("comando vazio")
	}
	executavel := comando[0]
	if filepath.Ext(executavel) == "" || !filepath.IsAbs(executavel) {
		if c, err := exec.LookPath(executavel); err == nil {
			executavel = c
		}
	}
	args := append([]string{executavel}, comando[1:]...)
	switch strings.ToLower(filepath.Ext(executavel)) {
	case ".cmd", ".bat":
		for _, a := range args {
			if strings.ContainsAny(a, "&|<>^%!\"\r\n") {
				return "", fmt.Errorf("argumento com caractere especial do cmd: %q", a)
			}
		}
		cmd := os.Getenv("ComSpec")
		if cmd == "" {
			cmd = `C:\Windows\System32\cmd.exe`
		}
		return windows.EscapeArg(cmd) + ` /d /s /c "` + windows.ComposeCommandLine(args) + `"`, nil
	}
	return windows.ComposeCommandLine(args), nil
}

// blocoDeAmbiente monta o bloco UTF-16 do CreateProcess: uma variável por
// entrada, sem repetir nome (sem diferença de maiúsculas, a última vale), em
// ordem, terminado por dois zeros.
func blocoDeAmbiente(env []string) ([]uint16, error) {
	porNome := map[string]string{}
	for _, v := range env {
		nome, _, ok := strings.Cut(v, "=")
		// "=C:=C:\pasta" (o diretório de cada unidade) começa com "=".
		if !ok || strings.ContainsRune(v, 0) {
			continue
		}
		if nome == "" {
			nome = v[:strings.IndexByte(v[1:], '=')+1]
		}
		porNome[strings.ToUpper(nome)] = v
	}
	chaves := make([]string, 0, len(porNome))
	for k := range porNome {
		chaves = append(chaves, k)
	}
	sort.Strings(chaves)
	var bloco []uint16
	for _, k := range chaves {
		bloco = append(bloco, utf16.Encode([]rune(porNome[k]))...)
		bloco = append(bloco, 0)
	}
	return append(bloco, 0), nil
}

// ShellPadrao é o shell de um agente "shell": o PowerShell, ou o cmd.exe.
func ShellPadrao() string {
	if c, err := exec.LookPath("powershell.exe"); err == nil {
		return c
	}
	if c := os.Getenv("ComSpec"); c != "" {
		return c
	}
	return `C:\Windows\System32\cmd.exe`
}
