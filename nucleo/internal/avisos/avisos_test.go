package avisos

import (
	"context"
	"encoding/json"
	"testing"
	"time"
)

func proxima(t *testing.T, a *Assinatura) map[string]any {
	t.Helper()
	ctx, cancelar := context.WithTimeout(context.Background(), time.Second)
	defer cancelar()
	bruto, err := a.Proxima(ctx)
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	json.Unmarshal(bruto, &m)
	return m
}

func TestCadaTelaSoRecebeOSeuPerfil(t *testing.T) {
	b := Novo()
	um, _ := b.Assinar(1)
	dois, _ := b.Assinar(2)
	b.Publicar(1, map[string]any{"tipo": "tarefa.criada"})
	b.Publicar(Todos, map[string]any{"tipo": "nucleo.aviso"})
	if m := proxima(t, um); m["tipo"] != "tarefa.criada" || m["seq"] != 1.0 {
		t.Errorf("primeira do perfil 1: %v", m)
	}
	if m := proxima(t, um); m["tipo"] != "nucleo.aviso" {
		t.Errorf("aviso geral no perfil 1: %v", m)
	}
	if m := proxima(t, dois); m["tipo"] != "nucleo.aviso" || m["seq"] != 2.0 {
		t.Errorf("o perfil 2 recebeu %v", m)
	}
	if b.Seq() != 2 {
		t.Errorf("seq %d", b.Seq())
	}
}

func TestTelaLentaNaoTravaQuemPublica(t *testing.T) {
	b := Novo()
	lenta, _ := b.Assinar(1)
	pronto := make(chan struct{})
	go func() {
		for range 10 * TamanhoFila {
			b.Publicar(1, map[string]any{"tipo": "agente.estado"})
		}
		close(pronto)
	}()
	select {
	case <-pronto:
	case <-time.After(5 * time.Second):
		t.Fatal("Publicar travou esperando uma tela que não lê")
	}
	if m := proxima(t, lenta); m["tipo"] != "recarregar" {
		t.Errorf("a tela atrasada deveria recarregar, recebeu %v", m)
	}
	// Depois do recarregar, segue normal.
	b.Publicar(1, map[string]any{"tipo": "tarefa.criada"})
	if m := proxima(t, lenta); m["tipo"] != "tarefa.criada" {
		t.Errorf("depois do recarregar: %v", m)
	}
}

func TestLimiteDeTelas(t *testing.T) {
	b := Novo()
	var ultima *Assinatura
	for range MaxAssinaturas {
		a, err := b.Assinar(1)
		if err != nil {
			t.Fatal(err)
		}
		ultima = a
	}
	if _, err := b.Assinar(1); err != ErrMuitasAssinaturas {
		t.Errorf("acima do limite: %v", err)
	}
	ultima.Cancelar()
	if _, err := b.Assinar(1); err != nil {
		t.Errorf("depois de uma sair: %v", err)
	}
}
