package voz

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

var conteudo = []byte(strings.Repeat("modelo de teste ", 4096))

func hashDe(b []byte) string {
	s := sha256.Sum256(b)
	return hex.EncodeToString(s[:])
}

// gerenteDeTeste serve `servido` num https de teste e espera `conteudo`.
func gerenteDeTeste(t *testing.T, servido []byte) (*Gerente, *httptest.Server) {
	t.Helper()
	srv := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write(servido)
	}))
	t.Cleanup(srv.Close)
	g := Novo(t.TempDir(), Modelo{Nome: "ggml-teste.bin", URL: srv.URL + "/m.bin", SHA256: hashDe(conteudo), Tamanho: int64(len(conteudo))})
	g.Cliente = srv.Client()
	g.Cliente.CheckRedirect = soHTTPS
	return g, srv
}

func esperar(t *testing.T, g *Gerente) error {
	t.Helper()
	fim := make(chan error, 1)
	if err := g.Baixar(func(err error) { fim <- err }); err != nil {
		t.Fatal(err)
	}
	select {
	case err := <-fim:
		return err
	case <-time.After(10 * time.Second):
		t.Fatal("o download não terminou")
		return nil
	}
}

func semTemporarios(t *testing.T, g *Gerente) {
	t.Helper()
	itens, _ := os.ReadDir(g.dir)
	for _, i := range itens {
		if strings.HasPrefix(i.Name(), ".baixando-") {
			t.Errorf("ficou o temporário %s", i.Name())
		}
	}
}

func TestBaixaConfereEGuardaSoParaOUsuario(t *testing.T) {
	g, _ := gerenteDeTeste(t, conteudo)
	if e := g.Estado(); e.Estado != Ausente || e.Tamanho != int64(len(conteudo)) || e.Caminho != "" {
		t.Fatalf("antes: %+v", e)
	}
	var estados []Estado
	g.AoMudar = func(e Estado) { estados = append(estados, e) }
	if err := esperar(t, g); err != nil {
		t.Fatal(err)
	}
	e := g.Estado()
	if e.Estado != Pronto || e.Caminho != g.Caminho() || e.Baixados != e.Tamanho {
		t.Fatalf("depois: %+v", e)
	}
	lido, _ := os.ReadFile(g.Caminho())
	if string(lido) != string(conteudo) {
		t.Error("o arquivo não é o que foi servido")
	}
	if runtime.GOOS != "windows" {
		if info, _ := os.Stat(g.Caminho()); info.Mode().Perm() != 0o600 {
			t.Errorf("permissão do modelo: %v", info.Mode().Perm())
		}
		if info, _ := os.Stat(g.dir); info.Mode().Perm() != 0o700 {
			t.Errorf("permissão da pasta: %v", info.Mode().Perm())
		}
	}
	semTemporarios(t, g)
	if len(estados) < 2 || estados[0].Estado != Baixando || estados[len(estados)-1].Estado != Pronto {
		t.Errorf("avisos de progresso: %+v", estados)
	}
	// Já pronto, baixar de novo não vai à rede e termina na hora.
	if err := esperar(t, g); err != nil {
		t.Error(err)
	}
}

func TestHashErradoERecusado(t *testing.T) {
	adulterado := append([]byte{}, conteudo...)
	adulterado[100] ^= 1
	g, _ := gerenteDeTeste(t, adulterado)
	err := esperar(t, g)
	if err == nil || !strings.Contains(err.Error(), "SHA-256") {
		t.Fatalf("aceitou um arquivo adulterado: %v", err)
	}
	if _, err := os.Stat(g.Caminho()); !errors.Is(err, os.ErrNotExist) {
		t.Error("o arquivo adulterado ficou no disco")
	}
	if e := g.Estado(); e.Estado != Falhou || !strings.Contains(e.Erro, "SHA-256") {
		t.Errorf("estado: %+v", e)
	}
	semTemporarios(t, g)
}

func TestTamanhoDiferenteERecusado(t *testing.T) {
	for nome, servido := range map[string][]byte{"maior": append(append([]byte{}, conteudo...), 'x'), "menor": conteudo[:len(conteudo)-1]} {
		t.Run(nome, func(t *testing.T) {
			g, _ := gerenteDeTeste(t, servido)
			if err := esperar(t, g); err == nil || !strings.Contains(err.Error(), "tamanho") {
				t.Fatalf("aceitou: %v", err)
			}
			if g.Pronto() {
				t.Error("ficou pronto")
			}
			semTemporarios(t, g)
		})
	}
}

func TestSoBaixaPorHTTPS(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.Write(conteudo) }))
	defer srv.Close()
	g := Novo(t.TempDir(), Modelo{Nome: "m.bin", URL: srv.URL, SHA256: hashDe(conteudo), Tamanho: int64(len(conteudo))})
	if err := esperar(t, g); err == nil || !strings.Contains(err.Error(), "https") {
		t.Fatalf("baixou por http: %v", err)
	}
	// Um https que redireciona para http também não serve.
	tls := httptest.NewTLSServer(http.RedirectHandler(srv.URL, http.StatusFound))
	defer tls.Close()
	g = Novo(t.TempDir(), Modelo{Nome: "m.bin", URL: tls.URL, SHA256: hashDe(conteudo), Tamanho: int64(len(conteudo))})
	g.Cliente = tls.Client()
	g.Cliente.CheckRedirect = soHTTPS
	if err := esperar(t, g); err == nil || g.Pronto() {
		t.Fatalf("seguiu o redirecionamento para http: %v", err)
	}
}

func TestCancelarApagaOParcial(t *testing.T) {
	soltar := make(chan struct{})
	srv := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Length", "65536")
		w.Write(conteudo[:1000])
		w.(http.Flusher).Flush()
		select {
		case <-r.Context().Done():
		case <-soltar:
		}
	}))
	defer srv.Close()
	defer close(soltar)
	g := Novo(t.TempDir(), Modelo{Nome: "m.bin", URL: srv.URL, SHA256: hashDe(conteudo), Tamanho: 65536})
	g.Cliente = srv.Client()
	fim := make(chan error, 1)
	if err := g.Baixar(func(err error) { fim <- err }); err != nil {
		t.Fatal(err)
	}
	if err := g.Baixar(func(error) {}); !errors.Is(err, ErrJaBaixando) {
		t.Errorf("dois downloads ao mesmo tempo: %v", err)
	}
	if err := g.Remover(); !errors.Is(err, ErrJaBaixando) {
		t.Errorf("removeu durante o download: %v", err)
	}
	for g.Estado().Baixados < 1000 {
		time.Sleep(5 * time.Millisecond)
	}
	if e := g.Estado(); e.Estado != Baixando {
		t.Errorf("durante: %+v", e)
	}
	if !g.Cancelar() {
		t.Fatal("não havia o que cancelar")
	}
	select {
	case err := <-fim:
		if !errors.Is(err, ErrCancelado) {
			t.Errorf("fim: %v", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("o cancelamento não parou o download")
	}
	if e := g.Estado(); e.Estado != Ausente || e.Erro != "" {
		t.Errorf("depois de cancelar: %+v", e)
	}
	semTemporarios(t, g)
	if g.Cancelar() {
		t.Error("cancelou sem download")
	}
}

func TestRemoverApagaOModelo(t *testing.T) {
	g, _ := gerenteDeTeste(t, conteudo)
	if err := esperar(t, g); err != nil {
		t.Fatal(err)
	}
	if err := g.Remover(); err != nil {
		t.Fatal(err)
	}
	if g.Pronto() || g.Estado().Estado != Ausente {
		t.Error("continuou pronto")
	}
	if err := g.Remover(); err != nil {
		t.Errorf("remover sem modelo: %v", err)
	}
}

func TestModeloBaseFixo(t *testing.T) {
	if !strings.HasPrefix(Base.URL, "https://huggingface.co/ggerganov/whisper.cpp/resolve/") || Base.Tamanho != 147951465 || len(Base.SHA256) != 64 {
		t.Errorf("modelo base: %+v", Base)
	}
	if filepath.Base(Novo("/d", Base).Caminho()) != "ggml-base.bin" {
		t.Error("nome do arquivo")
	}
}

// tgz monta um .tgz na memória com os membros dados (nome → conteúdo; um
// nome terminado em "@" vira link simbólico para o resto do nome).
func tgz(t *testing.T, membros map[string]string) []byte {
	t.Helper()
	var buf bytes.Buffer
	gz := gzip.NewWriter(&buf)
	tw := tar.NewWriter(gz)
	for nome, dados := range membros {
		if alvo, ok := strings.CutSuffix(nome, "@"); ok {
			if err := tw.WriteHeader(&tar.Header{Name: alvo, Typeflag: tar.TypeSymlink, Linkname: dados}); err != nil {
				t.Fatal(err)
			}
			continue
		}
		if err := tw.WriteHeader(&tar.Header{Name: nome, Mode: 0o755, Size: int64(len(dados)), Typeflag: tar.TypeReg}); err != nil {
			t.Fatal(err)
		}
		tw.Write([]byte(dados))
	}
	tw.Close()
	gz.Close()
	return buf.Bytes()
}

// servirVarios serve cada caminho com o conteúdo dado num https de teste.
func servirVarios(t *testing.T, arquivos map[string][]byte) *httptest.Server {
	t.Helper()
	srv := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		c, ok := arquivos[r.URL.Path]
		if !ok {
			http.NotFound(w, r)
			return
		}
		w.Write(c)
	}))
	t.Cleanup(srv.Close)
	return srv
}

func TestVariosArquivosEUmMembroDoPacote(t *testing.T) {
	biblioteca := strings.Repeat("biblioteca ", 500)
	pacote := tgz(t, map[string]string{"pacote/LICENSE": "MIT", "pacote/lib/libx.so.1": biblioteca, "pacote/lib/libx.so@": "libx.so.1"})
	srv := servirVarios(t, map[string][]byte{"/modelo.onnx": conteudo, "/voz.bin": []byte("voz"), "/pacote.tgz": pacote})
	g := Novo(t.TempDir(),
		Modelo{Nome: "modelo.onnx", URL: srv.URL + "/modelo.onnx", SHA256: hashDe(conteudo), Tamanho: int64(len(conteudo))},
		Modelo{Nome: "voz.bin", URL: srv.URL + "/voz.bin", SHA256: hashDe([]byte("voz")), Tamanho: 3},
		Modelo{Nome: "libx.so.1", URL: srv.URL + "/pacote.tgz", SHA256: hashDe(pacote), Tamanho: int64(len(pacote)),
			Extrair: "pacote/lib/libx.so.1", TamanhoFinal: int64(len(biblioteca)), SHA256Final: hashDe([]byte(biblioteca))},
	)
	g.Cliente = srv.Client()
	g.Cliente.CheckRedirect = soHTTPS
	if e := g.Estado(); e.Tamanho != int64(len(conteudo)+3+len(pacote)) {
		t.Errorf("o tamanho mostrado é o que se baixa: %+v", e)
	}
	if err := esperar(t, g); err != nil {
		t.Fatal(err)
	}
	e := g.Estado()
	if e.Estado != Pronto || len(e.Caminhos) != 3 || e.Caminho != e.Caminhos[0] {
		t.Fatalf("%+v", e)
	}
	if lido, _ := os.ReadFile(e.Caminhos[2]); string(lido) != biblioteca {
		t.Error("o membro extraído não é o do pacote")
	}
	// Do pacote, só o membro pedido fica na pasta.
	itens, _ := os.ReadDir(g.dir)
	if len(itens) != 3 {
		t.Errorf("sobrou coisa na pasta: %v", itens)
	}
	// Um arquivo que some faz baixar só ele de novo.
	os.Remove(e.Caminhos[1])
	if g.Pronto() {
		t.Error("pronto sem um dos arquivos")
	}
	if err := esperar(t, g); err != nil || !g.Pronto() {
		t.Fatal(err)
	}
	if err := g.Remover(); err != nil || g.Pronto() {
		t.Fatal(err)
	}
	if itens, _ := os.ReadDir(g.dir); len(itens) != 0 {
		t.Errorf("remover deixou %v", itens)
	}
}

func TestMembroDoPacoteErradoERecusado(t *testing.T) {
	biblioteca := "biblioteca"
	casos := map[string]map[string]string{
		"sem o membro":     {"pacote/outro": biblioteca},
		"membro diferente": {"pacote/lib/libx.so.1": "adulterada"},
		"link":             {"pacote/lib/libx.so.1@": "/etc/passwd"},
	}
	for nome, membros := range casos {
		t.Run(nome, func(t *testing.T) {
			pacote := tgz(t, membros)
			srv := servirVarios(t, map[string][]byte{"/p.tgz": pacote})
			g := Novo(t.TempDir(), Modelo{Nome: "libx.so.1", URL: srv.URL + "/p.tgz", SHA256: hashDe(pacote), Tamanho: int64(len(pacote)),
				Extrair: "pacote/lib/libx.so.1", TamanhoFinal: int64(len(biblioteca)), SHA256Final: hashDe([]byte(biblioteca))})
			g.Cliente = srv.Client()
			if err := esperar(t, g); err == nil || g.Pronto() {
				t.Fatalf("aceitou: %v", err)
			}
			if itens, _ := os.ReadDir(g.dir); len(itens) != 0 {
				t.Errorf("ficou %v", itens)
			}
		})
	}
}

func TestKokoroFixo(t *testing.T) {
	var total int64
	for _, m := range Kokoro {
		if !strings.HasPrefix(m.URL, "https://") || len(m.SHA256) != 64 || m.Tamanho <= 0 {
			t.Errorf("%+v", m)
		}
		if m.Extrair != "" && (len(m.SHA256Final) != 64 || m.TamanhoFinal <= 0) {
			t.Errorf("membro sem conferência: %+v", m)
		}
		total += m.Tamanho
	}
	// O que a tela mostra antes de baixar: uns 104 MB.
	if g := Novo("/d", Kokoro...); g.Estado().Tamanho != total || total != 104291359 {
		t.Errorf("total %d", total)
	}
}
