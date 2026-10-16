//go:build unix

package api

import (
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/voz"
)

// servidorComVoz serve `servido` como modelo de voz num https de teste; o
// esperado é sempre `modeloDeTeste`.
func servidorComVoz(t *testing.T, servido []byte) *httptest.Server {
	t.Helper()
	fonte := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.Write(servido) }))
	t.Cleanup(fonte.Close)
	dir := t.TempDir()
	banco, err := dados.Abrir(filepath.Join(dir, "dados"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { banco.Fechar() })
	soma := sha256.Sum256(modeloDeTeste)
	g := voz.Novo(dir, voz.Modelo{Nome: "ggml-teste.bin", URL: fonte.URL + "/m.bin", SHA256: hex.EncodeToString(soma[:]), Tamanho: int64(len(modeloDeTeste))})
	g.Cliente = fonte.Client()
	servidor := &Servidor{Bytes: new(atomic.Int64), Banco: banco, DirDados: dir, Versao: "teste", Tempos: temposDeTeste, Voz: g}
	srv := httptest.NewServer(servidor.Rotas())
	t.Cleanup(func() { servidor.Encerrar() })
	t.Cleanup(srv.Close)
	return srv
}

var modeloDeTeste = []byte(strings.Repeat("whisper ", 20000))

func ditadoDoPerfil(t *testing.T, url string, id float64) bool {
	t.Helper()
	_, perfis := pedirLista(t, url+"/v1/perfis")
	for _, p := range perfis {
		if p["id"] == id {
			return p["ditado"] == true
		}
	}
	t.Fatalf("perfil %v sumiu", id)
	return false
}

func TestDitadoLigaSoComOModeloBaixado(t *testing.T) {
	srv := servidorComVoz(t, modeloDeTeste)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	if ditadoDoPerfil(t, srv.URL, 1) {
		t.Fatal("o ditado nasce desligado")
	}
	if _, e := pedir(t, "GET", srv.URL+"/v1/voz", nil); e["estado"] != "ausente" || e["tamanho"] != float64(len(modeloDeTeste)) {
		t.Fatalf("estado antes: %v", e)
	}
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"ditado": true}); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "Baixe o modelo") {
		t.Fatalf("ligou sem o modelo: %d %v", status, r)
	}

	eventos := ouvir(t, srv.URL, "1")
	if status, r := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/baixar", nil); status != http.StatusOK {
		t.Fatalf("baixar: %d %v", status, r)
	}
	eventos.esperar("voz.modelo pronto", func(m map[string]any) bool { return m["tipo"] == "voz.modelo" && m["estado"] == "pronto" })
	if m := eventos.esperar("perfil.ditado", func(m map[string]any) bool { return m["tipo"] == "perfil.ditado" }); m["ligado"] != true || m["perfil_id"] != 1.0 {
		t.Errorf("evento do ditado: %v", m)
	}
	if !ditadoDoPerfil(t, srv.URL, 1) || ditadoDoPerfil(t, srv.URL, 2) {
		t.Error("o download liga o ditado só de quem pediu")
	}
	_, e := pedir(t, "GET", srv.URL+"/v1/voz", nil)
	if e["estado"] != "pronto" || !strings.HasSuffix(e["caminho"].(string), filepath.Join("voz", "ggml-teste.bin")) {
		t.Fatalf("estado depois: %v", e)
	}
	// Com o modelo no disco, o outro perfil liga e desliga direto.
	if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/2", map[string]any{"ditado": true}); status != http.StatusOK {
		t.Errorf("ligar com o modelo: %d", status)
	}
	if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"ditado": "sim"}); status != http.StatusBadRequest {
		t.Errorf("ditado que não é booleano: %d", status)
	}

	// Apagar o modelo desliga o ditado de todos.
	if status, r := pedir(t, "DELETE", srv.URL+"/v1/voz/modelo", nil); status != http.StatusOK || r["estado"] != "ausente" {
		t.Fatalf("apagar: %d %v", status, r)
	}
	if ditadoDoPerfil(t, srv.URL, 1) || ditadoDoPerfil(t, srv.URL, 2) {
		t.Error("o ditado continuou ligado sem o modelo")
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/99/voz/baixar", nil); status != http.StatusNotFound {
		t.Errorf("perfil que não existe: %d", status)
	}
	if _, r := pedir(t, "POST", srv.URL+"/v1/voz/cancelar", nil); r["cancelado"] != false {
		t.Errorf("cancelar sem download: %v", r)
	}
}

func TestModeloAdulteradoNaoLigaODitado(t *testing.T) {
	adulterado := append([]byte{}, modeloDeTeste...)
	adulterado[0] = 'W'
	srv := servidorComVoz(t, adulterado)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	eventos := ouvir(t, srv.URL, "1")
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/baixar", nil); status != http.StatusOK {
		t.Fatalf("baixar: %d", status)
	}
	m := eventos.esperar("voz.modelo erro", func(m map[string]any) bool { return m["tipo"] == "voz.modelo" && m["estado"] == "erro" })
	if !strings.Contains(m["erro"].(string), "SHA-256") {
		t.Errorf("erro do download: %v", m)
	}
	if ditadoDoPerfil(t, srv.URL, 1) {
		t.Error("ligou o ditado com um modelo recusado")
	}
	if _, e := pedir(t, "GET", srv.URL+"/v1/voz", nil); e["estado"] != "erro" || e["caminho"] != nil {
		t.Errorf("estado: %v", e)
	}
}
