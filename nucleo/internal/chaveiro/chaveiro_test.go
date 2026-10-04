package chaveiro

import (
	"errors"
	"sync"
	"testing"
	"time"

	"github.com/zalando/go-keyring"
)

// falso imita o chaveiro do sistema; com travar, nunca responde.
type falso struct {
	mu     sync.Mutex
	travar chan struct{}
	senhas map[string]string
	erro   error
}

func (f *falso) esperar() {
	if f.travar != nil {
		<-f.travar
	}
}

func (f *falso) Set(s, c, p string) error {
	f.esperar()
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.erro != nil {
		return f.erro
	}
	f.senhas[s+"/"+c] = p
	return nil
}

func (f *falso) Get(s, c string) (string, error) {
	f.esperar()
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.erro != nil {
		return "", f.erro
	}
	v, ok := f.senhas[s+"/"+c]
	if !ok {
		return "", keyring.ErrNotFound
	}
	return v, nil
}

func (f *falso) Delete(s, c string) error {
	f.esperar()
	f.mu.Lock()
	defer f.mu.Unlock()
	if _, ok := f.senhas[s+"/"+c]; !ok {
		return keyring.ErrNotFound
	}
	delete(f.senhas, s+"/"+c)
	return nil
}

func TestSistemaComFalso(t *testing.T) {
	f := &falso{senhas: map[string]string{}}
	s := &Sistema{Servico: "colmeia-teste", Prazo: time.Second, Ops: f}
	if !s.Disponivel() {
		t.Fatal("o chaveiro falso responde: está disponível")
	}
	if err := s.Guardar("conexao:1", "canario"); err != nil {
		t.Fatal(err)
	}
	if v, err := s.Ler("conexao:1"); err != nil || v != "canario" {
		t.Fatalf("ler: %q %v", v, err)
	}
	if _, err := s.Ler("conexao:2"); !errors.Is(err, ErrNaoEncontrada) {
		t.Fatalf("conta sem senha: %v", err)
	}
	if err := s.Apagar("conexao:1"); err != nil {
		t.Fatal(err)
	}
	if err := s.Apagar("conexao:1"); err != nil {
		t.Fatalf("apagar o que não existe não é erro: %v", err)
	}
	if len(f.senhas) != 0 {
		t.Fatalf("sobrou: %v", f.senhas)
	}
}

func TestSistemaIndisponivel(t *testing.T) {
	f := &falso{senhas: map[string]string{}, erro: errors.New("sem D-Bus")}
	s := &Sistema{Servico: "colmeia-teste", Prazo: time.Second, Ops: f}
	if s.Disponivel() {
		t.Fatal("sem D-Bus não há chaveiro")
	}
	if err := s.Guardar("conexao:1", "x"); !errors.Is(err, ErrIndisponivel) {
		t.Fatalf("guardar sem chaveiro: %v", err)
	}
}

func TestSistemaTravadoRespeitaPrazo(t *testing.T) {
	f := &falso{senhas: map[string]string{}, travar: make(chan struct{})}
	defer close(f.travar)
	s := &Sistema{Servico: "colmeia-teste", Prazo: 50 * time.Millisecond, Ops: f}
	inicio := time.Now()
	if s.Disponivel() {
		t.Fatal("travado na sondagem: indisponível")
	}
	if time.Since(inicio) > time.Second {
		t.Fatal("não respeitou o prazo")
	}
	// Já sondado: sem nova espera.
	inicio = time.Now()
	if _, err := s.Ler("x"); !errors.Is(err, ErrIndisponivel) || time.Since(inicio) > 20*time.Millisecond {
		t.Fatalf("depois da sondagem: %v", err)
	}
}

func TestMemoria(t *testing.T) {
	m := NovaMemoria()
	if !m.Disponivel() {
		t.Fatal("memória funciona como chaveiro")
	}
	m.Guardar("a", "1")
	if v, _ := m.Ler("a"); v != "1" {
		t.Fatal(v)
	}
	m.Apagar("a")
	if _, err := m.Ler("a"); !errors.Is(err, ErrNaoEncontrada) {
		t.Fatal(err)
	}
	ausente := &Memoria{Ausente: true}
	if ausente.Disponivel() || ausente.Guardar("a", "1") == nil {
		t.Fatal("ausente faz o papel de sem chaveiro")
	}
}
