//go:build windows

package terminal

import (
	"bytes"
	"io"
	"strings"
	"testing"
	"time"
)

// lerAte junta a saída até achar o texto ou o tempo acabar.
func lerAte(t *testing.T, p Pty, texto string) string {
	t.Helper()
	var visto bytes.Buffer
	pronto := make(chan struct{})
	go func() {
		buf := make([]byte, 4096)
		for {
			n, err := p.Read(buf)
			visto.Write(buf[:n])
			if strings.Contains(visto.String(), texto) {
				close(pronto)
				io.Copy(io.Discard, p)
				return
			}
			if err != nil {
				close(pronto)
				return
			}
		}
	}()
	select {
	case <-pronto:
	case <-time.After(20 * time.Second):
		t.Fatalf("não apareceu %q na saída", texto)
	}
	return visto.String()
}

func TestConPTYRodaEDizComoSaiu(t *testing.T) {
	p, err := Iniciar([]string{"cmd.exe", "/c", "echo colmeia-ok && exit 3"}, []string{"COLMEIA_TESTE=1"}, t.TempDir(), Tamanho{Colunas: 100, Linhas: 30})
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	if saida := lerAte(t, p, "colmeia-ok"); !strings.Contains(saida, "colmeia-ok") {
		t.Fatalf("saída: %q", saida)
	}
	if s := p.Esperar(); s.Codigo != 3 {
		t.Errorf("código %d, esperado 3", s.Codigo)
	}
}

func TestConPTYRecebeDigitacaoERedimensiona(t *testing.T) {
	p, err := Iniciar([]string{"cmd.exe", "/q", "/k"}, nil, t.TempDir(), TamanhoPadrao)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	if err := p.Redimensionar(120, 40); err != nil {
		t.Errorf("redimensionar: %v", err)
	}
	if _, err := p.Write([]byte("echo digitado-%COLMEIA_X%ok\r")); err != nil {
		t.Fatal(err)
	}
	lerAte(t, p, "digitado-")
}

func TestFecharEncerraOProgramaQueNaoSai(t *testing.T) {
	p, err := Iniciar([]string{"cmd.exe", "/c", "ping -n 60 127.0.0.1"}, nil, t.TempDir(), TamanhoPadrao)
	if err != nil {
		t.Fatal(err)
	}
	go io.Copy(io.Discard, p)
	inicio := time.Now()
	p.Close()
	if time.Since(inicio) > 15*time.Second {
		t.Errorf("fechar demorou %v", time.Since(inicio))
	}
	p.Esperar()
}

func TestLinhaDeComandoDoCmdRecusaCaractereEspecial(t *testing.T) {
	if _, err := linhaDeComando([]string{`C:\npm\claude.cmd`, "--resume", "a&calc"}); err == nil {
		t.Error("um .cmd com & no argumento deveria ser recusado")
	}
	linha, err := linhaDeComando([]string{`C:\npm\claude.cmd`, "--resume", "abc"})
	if err != nil || !strings.Contains(linha, `/d /s /c "C:\npm\claude.cmd --resume abc"`) {
		t.Errorf("linha: %q, %v", linha, err)
	}
}

func TestBlocoDeAmbienteSemRepetir(t *testing.T) {
	bloco, _ := blocoDeAmbiente([]string{"Path=a", "PATH=b", "TERM=x"})
	texto := string(func() []rune {
		r := make([]rune, len(bloco))
		for i, c := range bloco {
			r[i] = rune(c)
		}
		return r
	}())
	if strings.Count(strings.ToUpper(texto), "PATH=") != 1 || !strings.Contains(texto, "PATH=b") {
		t.Errorf("bloco: %q", texto)
	}
	if !strings.HasSuffix(texto, "\x00\x00") {
		t.Error("o bloco termina com dois zeros")
	}
}
