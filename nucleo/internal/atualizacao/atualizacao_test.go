package atualizacao

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

func chaves(t *testing.T) (string, ed25519.PrivateKey) {
	t.Helper()
	pub, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return base64.StdEncoding.EncodeToString(pub), priv
}

func TestSoValeOManifestoAssinado(t *testing.T) {
	pub, priv := chaves(t)
	outra, _ := chaves(t)
	bruto := []byte(`{"versao":"0.5.0","notas":"- novo","arquivos":{"colmeia-linux-x86_64.tar.gz":{"sha256":"` + strings.Repeat("a", 64) + `","tamanho":10}}}`)
	sig := Assinar(priv, bruto)
	m, err := Verificar(pub, bruto, sig)
	if err != nil || m.Versao != "0.5.0" || m.Notas != "- novo" {
		t.Fatalf("assinado: %+v %v", m, err)
	}
	mexido := bytes.Replace(bruto, []byte("0.5.0"), []byte("9.9.9"), 1)
	if _, err := Verificar(pub, mexido, sig); err != ErrAssinatura {
		t.Fatalf("mexido passou: %v", err)
	}
	if _, err := Verificar(outra, bruto, sig); err != ErrAssinatura {
		t.Fatalf("outra chave passou: %v", err)
	}
	// Uma assinatura do manifesto sem o rótulo não serve.
	if _, err := Verificar(pub, bruto, base64.StdEncoding.EncodeToString(ed25519.Sign(priv, bruto))); err != ErrAssinatura {
		t.Fatalf("sem o rótulo passou: %v", err)
	}
	foraDoFormato := []byte(`{"versao":"0.5","arquivos":{}}`)
	if _, err := Verificar(pub, foraDoFormato, Assinar(priv, foraDoFormato)); err == nil {
		t.Fatal("versão fora do formato passou")
	}
	nomeRuim := []byte(`{"versao":"0.5.0","arquivos":{"../x":{"sha256":"` + strings.Repeat("a", 64) + `","tamanho":1}}}`)
	if _, err := Verificar(pub, nomeRuim, Assinar(priv, nomeRuim)); err == nil {
		t.Fatal("nome com caminho passou")
	}
	if _, err := base64.StdEncoding.DecodeString(ChavePublica); err != nil {
		t.Fatalf("a chave pública da Colmeia não é base64: %v", err)
	}
}

func TestMaisNova(t *testing.T) {
	casos := []struct {
		a, b string
		quer bool
	}{
		{"0.5.0", "0.4.0", true}, {"0.10.0", "0.9.3", true}, {"1.0.0", "0.99.99", true}, {"v0.4.1", "0.4.0", true},
		{"0.4.0", "0.4.0", false}, {"0.3.9", "0.4.0", false}, {"0.5", "0.4.0", false}, {"0.5.0-rc1", "0.4.0", false},
	}
	for _, c := range casos {
		if MaisNova(c.a, c.b) != c.quer {
			t.Errorf("MaisNova(%q, %q) = %v", c.a, c.b, !c.quer)
		}
	}
}

func TestNotasDoChangelog(t *testing.T) {
	changelog := "# Mudanças\n\n## [Não lançado]\n\n- depois\n\n## [0.5.0] - 2026-10-20\n\n### Novo\n\n- **Atualizar:** a Colmeia avisa.\n\n## [0.4.0]\n\n- antes\n"
	notas, achou := NotasDoChangelog(changelog, "0.5.0")
	if !achou || notas != "### Novo\n\n- **Atualizar:** a Colmeia avisa." {
		t.Fatalf("%v %q", achou, notas)
	}
	if _, achou := NotasDoChangelog(changelog, "0.6.0"); achou {
		t.Fatal("achou uma versão que não existe")
	}
}

// release monta um pacote com os programas falsos (o núcleo responde
// --versao) e serve o manifesto assinado.
type release struct {
	pacote, manifesto []byte
	sig               string
}

func programaFalso(versao string) []byte {
	return []byte("#!/bin/sh\n[ \"$1\" = --versao ] && echo colmeia-nucleo " + versao + "\n")
}

func montarPacote(t *testing.T, membros map[string][]byte, extras ...*tar.Header) []byte {
	t.Helper()
	var buf bytes.Buffer
	gz := gzip.NewWriter(&buf)
	tw := tar.NewWriter(gz)
	for _, h := range extras {
		tw.WriteHeader(h)
	}
	for nome, conteudo := range membros {
		tw.WriteHeader(&tar.Header{Name: "./" + nome, Mode: 0o755, Size: int64(len(conteudo)), Typeflag: tar.TypeReg})
		tw.Write(conteudo)
	}
	tw.Close()
	gz.Close()
	return buf.Bytes()
}

func novaRelease(t *testing.T, priv ed25519.PrivateKey, versao string, pacote []byte) release {
	t.Helper()
	soma := sha256.Sum256(pacote)
	m := Manifesto{Versao: versao, Notas: "- **Novo:** atualizar.", Arquivos: map[string]Arquivo{
		"colmeia-linux-x86_64.tar.gz": {SHA256: hex.EncodeToString(soma[:]), Tamanho: int64(len(pacote))},
	}}
	bruto, _ := json.Marshal(m)
	return release{pacote: pacote, manifesto: bruto, sig: Assinar(priv, bruto)}
}

func servir(t *testing.T, r *release) *httptest.Server {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		switch req.URL.Path {
		case "/" + NomeManifesto:
			w.Write(r.manifesto)
		case "/" + NomeAssinatura:
			w.Write([]byte(r.sig))
		case "/colmeia-linux-x86_64.tar.gz":
			w.Write(r.pacote)
		default:
			http.NotFound(w, req)
		}
	}))
	t.Cleanup(srv.Close)
	return srv
}

// instalacao cria a pasta dos programas de agora e o gerente apontando para o servidor.
func instalacao(t *testing.T, pub string, srv *httptest.Server) *Gerente {
	t.Helper()
	if runtime.GOOS != "linux" || runtime.GOARCH != "amd64" {
		t.Skip("a troca automática é só do Linux x86-64")
	}
	raiz := t.TempDir()
	dirBin, dirDados := filepath.Join(raiz, "bin"), filepath.Join(raiz, "dados")
	os.MkdirAll(dirBin, 0o700)
	for _, p := range programas {
		os.WriteFile(filepath.Join(dirBin, p.nome), programaFalso("0.4.0"), 0o755)
	}
	g := Novo("0.4.0", dirDados, dirBin)
	g.Fonte, g.Chave, g.Cliente = srv.URL+"/", pub, srv.Client()
	g.CopiarBanco = func(destino string) error { return os.WriteFile(destino, []byte("banco de agora"), 0o600) }
	return g
}

func programasNovos() map[string][]byte {
	return map[string][]byte{
		"colmeia": []byte("tela 0.5.0"), "colmeia-nucleo": programaFalso("0.5.0"), "colmeia-abelha": []byte("abelha 0.5.0"),
	}
}

func TestProcuraEPreparaAVersaoNova(t *testing.T) {
	pub, priv := chaves(t)
	r := novaRelease(t, priv, "0.5.0", montarPacote(t, programasNovos(),
		// Fora da lista ou com caminho, fica de fora.
		&tar.Header{Name: "../../fora", Mode: 0o644, Size: 0, Typeflag: tar.TypeReg},
		&tar.Header{Name: "LICENSE-MIT", Mode: 0o644, Size: 0, Typeflag: tar.TypeReg}))
	srv := servir(t, &r)
	g := instalacao(t, pub, srv)

	var estados []string
	g.AoMudar = func(e Estado) { estados = append(estados, e.Estado) }
	g.Procurar(context.Background())
	e := g.Estado()
	if e.Estado != Disponivel || e.Versao != "0.5.0" || !e.Instalavel || e.Notas == "" || !e.Procurar {
		t.Fatalf("depois de procurar: %+v", e)
	}
	if err := g.Preparar(context.Background()); err != nil {
		t.Fatal(err)
	}
	if e := g.Estado(); e.Estado != Pronta || e.Baixados != int64(len(r.pacote)) {
		t.Fatalf("depois de preparar: %+v", e)
	}
	for nome := range programasNovos() {
		if _, err := os.Stat(filepath.Join(g.Dir, "nova", nome)); err != nil {
			t.Errorf("faltou %s na pasta nova: %v", nome, err)
		}
	}
	if entradas, _ := os.ReadDir(filepath.Join(g.Dir, "nova")); len(entradas) != 3 {
		t.Errorf("a pasta nova tem %d arquivos", len(entradas))
	}
	if _, err := os.Stat(filepath.Join(filepath.Dir(g.Dir), "..", "fora")); err == nil {
		t.Error("o pacote escreveu fora da pasta")
	}
	for _, nome := range []string{"colmeia", "colmeia-nucleo", "colmeia-abelha", "colmeia-voz", "colmeia.db"} {
		if _, err := os.Stat(filepath.Join(g.Dir, "anterior", nome)); err != nil {
			t.Errorf("faltou %s na cópia de agora: %v", nome, err)
		}
	}
	var p pendente
	bruto, _ := os.ReadFile(filepath.Join(g.Dir, "pendente.json"))
	if json.Unmarshal(bruto, &p); p.De != "0.4.0" || p.Para != "0.5.0" || p.DirBin != g.DirBin {
		t.Fatalf("pendente: %+v", p)
	}
	// Os programas de agora continuam os mesmos até a troca.
	if atual, _ := os.ReadFile(filepath.Join(g.DirBin, "colmeia")); !bytes.Equal(atual, programaFalso("0.4.0")) {
		t.Fatal("preparar já trocou um programa")
	}
	if estados[0] != Disponivel || estados[len(estados)-1] != Pronta {
		t.Fatalf("estados: %v", estados)
	}
}

func TestNaoInstalaOQueNaoConfere(t *testing.T) {
	pub, priv := chaves(t)
	bom := montarPacote(t, programasNovos())
	casos := map[string]func(*release){
		"pacote trocado": func(r *release) { r.pacote = montarPacote(t, map[string][]byte{"colmeia": []byte("outro")}) },
		"sem um programa": func(r *release) {
			*r = novaRelease(t, priv, "0.5.0", montarPacote(t, map[string][]byte{"colmeia": []byte("x"), "colmeia-nucleo": programaFalso("0.5.0")}))
		},
		"núcleo de outra versão": func(r *release) {
			p := programasNovos()
			p["colmeia-nucleo"] = programaFalso("0.4.0")
			*r = novaRelease(t, priv, "0.5.0", montarPacote(t, p))
		},
		"link no lugar do programa": func(r *release) {
			p := programasNovos()
			delete(p, "colmeia")
			*r = novaRelease(t, priv, "0.5.0", montarPacote(t, p, &tar.Header{Name: "colmeia", Linkname: "/etc/passwd", Typeflag: tar.TypeSymlink}))
		},
	}
	for nome, estragar := range casos {
		t.Run(nome, func(t *testing.T) {
			r := novaRelease(t, priv, "0.5.0", bom)
			srv := servir(t, &r)
			g := instalacao(t, pub, srv)
			g.Procurar(context.Background())
			estragar(&r)
			if err := g.Preparar(context.Background()); err == nil {
				t.Fatal("preparou")
			}
			if e := g.Estado(); e.Estado != Falhou || e.Erro == "" {
				t.Fatalf("%+v", e)
			}
			if _, err := os.Stat(filepath.Join(g.Dir, "nova")); err == nil {
				t.Fatal("sobrou a pasta nova")
			}
			if _, err := os.Stat(filepath.Join(g.Dir, "pendente.json")); err == nil {
				t.Fatal("ficou uma troca pendente")
			}
		})
	}
}

func TestSemNovidadeNemAssinaturaNaoMostraNada(t *testing.T) {
	pub, priv := chaves(t)
	_, outra := chaves(t)
	casos := map[string]release{
		"mesma versão":             novaRelease(t, priv, "0.4.0", montarPacote(t, programasNovos())),
		"versão anterior":          novaRelease(t, priv, "0.3.0", montarPacote(t, programasNovos())),
		"assinada por outra chave": novaRelease(t, outra, "0.5.0", montarPacote(t, programasNovos())),
	}
	for nome, r := range casos {
		t.Run(nome, func(t *testing.T) {
			srv := servir(t, &r)
			g := instalacao(t, pub, srv)
			g.Procurar(context.Background())
			if e := g.Estado(); e.Estado != Atualizada {
				t.Fatalf("%+v", e)
			}
		})
	}
}

func TestAVersaoQueFalhouNaoVoltaAAparecer(t *testing.T) {
	pub, priv := chaves(t)
	r := novaRelease(t, priv, "0.5.0", montarPacote(t, programasNovos()))
	srv := servir(t, &r)
	g := instalacao(t, pub, srv)
	os.MkdirAll(g.Dir, 0o700)
	os.WriteFile(filepath.Join(g.Dir, "falhou"), []byte("0.5.0\n"), 0o600)
	g.Procurar(context.Background())
	if e := g.Estado(); e.Estado != Atualizada {
		t.Fatalf("ofereceu a que falhou: %+v", e)
	}
	r2 := novaRelease(t, priv, "0.5.1", montarPacote(t, programasNovos()))
	r = r2
	g.procurado = time.Time{}
	g.Procurar(context.Background())
	if e := g.Estado(); e.Estado != Disponivel || e.Versao != "0.5.1" {
		t.Fatalf("não ofereceu a seguinte: %+v", e)
	}
}

func TestDesligadoNaoProcura(t *testing.T) {
	pub, _ := chaves(t)
	pedidos := 0
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) { pedidos++ }))
	t.Cleanup(srv.Close)
	g := instalacao(t, pub, srv)
	if err := g.GravarOpcoes(Opcoes{Procurar: false}); err != nil {
		t.Fatal(err)
	}
	g.Procurar(context.Background())
	if pedidos != 0 || g.Estado().Procurar {
		t.Fatalf("procurou desligado: %d pedidos", pedidos)
	}
}

func TestSoHTTPSForaDoComputador(t *testing.T) {
	g := Novo("0.4.0", t.TempDir(), "")
	g.Fonte = "http://example.com/"
	if _, err := g.lerManifesto(context.Background()); err == nil || !strings.Contains(err.Error(), "https") {
		t.Fatalf("aceitou http de fora: %v", err)
	}
}

func TestTrocaEVoltaOsProgramasEOBanco(t *testing.T) {
	raiz := t.TempDir()
	origem, dirBin, dirDados := filepath.Join(raiz, "nova"), filepath.Join(raiz, "bin"), filepath.Join(raiz, "dados")
	for _, d := range []string{origem, dirBin, dirDados} {
		os.MkdirAll(d, 0o700)
	}
	for nome, conteudo := range programasNovos() {
		os.WriteFile(filepath.Join(origem, nome), conteudo, 0o700)
		os.WriteFile(filepath.Join(dirBin, nome), []byte("antigo"), 0o755)
	}
	os.WriteFile(filepath.Join(dirBin, "colmeia-voz"), []byte("voz antiga"), 0o755)
	if err := trocarProgramas(origem, dirBin); err != nil {
		t.Fatal(err)
	}
	if b, _ := os.ReadFile(filepath.Join(dirBin, "colmeia")); string(b) != "tela 0.5.0" {
		t.Fatalf("não trocou: %q", b)
	}
	// O pacote sem a voz deixa a voz de antes.
	if b, _ := os.ReadFile(filepath.Join(dirBin, "colmeia-voz")); string(b) != "voz antiga" {
		t.Fatalf("mexeu na voz: %q", b)
	}
	if info, _ := os.Stat(filepath.Join(dirBin, "colmeia")); info.Mode().Perm() != 0o755 {
		t.Fatalf("permissão %v", info.Mode())
	}
	if sobra, _ := filepath.Glob(filepath.Join(dirBin, ".*.novo")); len(sobra) != 0 {
		t.Fatalf("sobrou temporário: %v", sobra)
	}

	banco := filepath.Join(dirDados, "colmeia.db")
	os.WriteFile(banco, []byte("migrado"), 0o600)
	os.WriteFile(banco+"-wal", []byte("wal"), 0o600)
	copia := filepath.Join(raiz, "copia.db")
	os.WriteFile(copia, []byte("de antes"), 0o600)
	if err := devolverBanco(copia, dirDados); err != nil {
		t.Fatal(err)
	}
	if b, _ := os.ReadFile(banco); string(b) != "de antes" {
		t.Fatalf("banco: %q", b)
	}
	if _, err := os.Stat(banco + "-wal"); err == nil {
		t.Fatal("ficou o -wal do banco migrado")
	}
}

func TestAvisoDaUltimaTrocaUmaVez(t *testing.T) {
	dirDados := t.TempDir()
	dir := filepath.Join(dirDados, "atualizacao")
	os.MkdirAll(dir, 0o700)
	bruto, _ := json.Marshal(resultado{OK: false, De: "0.4.0", Para: "0.5.0", Erro: "não respondeu"})
	// O resultado chega depois de o núcleo novo abrir.
	g := Novo("0.4.0", dirDados, "")
	os.WriteFile(filepath.Join(dir, "resultado.json"), bruto, 0o600)
	if a := g.VerAviso(); !strings.Contains(a, "voltou para a 0.4.0") {
		t.Fatalf("aviso: %q", a)
	}
	if a := g.VerAviso(); a != "" {
		t.Fatalf("de novo: %q", a)
	}
	if _, err := os.Stat(filepath.Join(dir, "resultado.json")); err == nil {
		t.Fatal("o resultado ficou no disco")
	}
}
