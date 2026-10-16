//go:build unix

package arquivos

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// repositorioDaTarefa: main com dois arquivos, a branch da tarefa muda um,
// apaga outro e cria um novo (um em commit, outro fora do git).
func repositorioDaTarefa(t *testing.T) string {
	t.Helper()
	pasta := filepath.Join(t.TempDir(), "loja-web")
	os.MkdirAll(filepath.Join(pasta, "src"), 0o700)
	git := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", append([]string{"-C", pasta, "-c", "user.email=teste@exemplo", "-c", "user.name=Teste"}, args...)...)
		if saida, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, saida)
		}
	}
	escrever := func(nome, conteudo string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(pasta, nome), []byte(conteudo), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	git("init", "-q", "-b", "main")
	escrever("src/pedidos.go", "package src\n\nfunc Total() int {\n\treturn 1\n}\n")
	escrever("src/velho.go", "package src\n")
	escrever(".gitignore", "build/\n")
	git("add", ".")
	git("commit", "-q", "-m", "início")
	git("checkout", "-q", "-b", "feature/pedidos")
	escrever("src/pedidos.go", "package src\n\nfunc Total() int {\n\treturn 2 // TotalDoPedido\n}\n")
	os.Remove(filepath.Join(pasta, "src", "velho.go"))
	escrever("src/tela_pedidos.go", "package src\n// tela nova\n")
	git("add", ".")
	git("commit", "-q", "-m", "pedidos")
	escrever("rascunho.txt", "anotação do pedido\nsegunda\n")
	escrever(".env", "SENHA_PEDIDO=x\n")
	os.MkdirAll(filepath.Join(pasta, "build"), 0o700)
	escrever("build/pedidos.out", "pedido gerado\n")
	return pasta
}

func caminhosDe(nomes []Nome) []string {
	var lista []string
	for _, n := range nomes {
		lista = append(lista, n.Caminho)
	}
	return lista
}

func TestBuscaPorNomeOrdenaPeloNomeDoArquivo(t *testing.T) {
	ctx := context.Background()
	pasta := repositorioDaTarefa(t)
	nomes, mais, err := BuscarNome(ctx, pasta, "pedidos")
	if err != nil || mais {
		t.Fatal(err, mais)
	}
	got := strings.Join(caminhosDe(nomes), ",")
	// O nome que começa com o termo vem antes; o ignorado pelo git não aparece.
	if got != "src/pedidos.go,src/tela_pedidos.go" {
		t.Errorf("busca por nome: %s", got)
	}
	// As letras em ordem também servem (tp → tela_pedidos).
	nomes, _, _ = BuscarNome(ctx, pasta, "tlpd")
	if len(nomes) != 1 || nomes[0].Caminho != "src/tela_pedidos.go" {
		t.Errorf("letras em ordem: %v", caminhosDe(nomes))
	}
	if _, _, err := BuscarNome(ctx, pasta, "a\nb"); !errors.Is(err, ErrTermo) {
		t.Errorf("termo com quebra de linha: %v", err)
	}
	if _, _, err := BuscarNome(ctx, pasta, strings.Repeat("a", MaxTermo+1)); !errors.Is(err, ErrTermo) {
		t.Errorf("termo longo: %v", err)
	}
}

func TestBuscaPorNomeSemGitPercorreAPasta(t *testing.T) {
	pasta, _ := pastaDeTeste(t)
	nomes, _, err := BuscarNome(context.Background(), pasta, "main")
	if err != nil {
		t.Fatal(err)
	}
	if got := strings.Join(caminhosDe(nomes), ","); got != "src/main.go" {
		t.Errorf("sem git: %s", got)
	}
}

func TestBuscaPorTextoComLinhaETrechoSemSegredos(t *testing.T) {
	ctx := context.Background()
	pasta := repositorioDaTarefa(t)
	achados, mais, err := BuscarTexto(ctx, pasta, "pedido", false)
	if err != nil || mais {
		t.Fatal(err, mais)
	}
	var got []string
	for _, a := range achados {
		got = append(got, a.Caminho+":"+a.Texto)
		if a.Linha < 1 {
			t.Errorf("sem linha: %+v", a)
		}
	}
	// O .env (senha) e o build/ (ignorado) ficam de fora; o novo fora do git entra.
	if strings.Join(got, "|") != "rascunho.txt:anotação do pedido|src/pedidos.go:return 2 // TotalDoPedido" {
		t.Errorf("achados: %v", got)
	}
	if achados, _, _ := BuscarTexto(ctx, pasta, "PEDIDO", true); len(achados) != 0 {
		t.Errorf("diferenciando maiúsculas: %v", achados)
	}
	// Um termo que parece opção do git é só texto.
	if _, _, err := BuscarTexto(ctx, pasta, "--help", false); err != nil {
		t.Errorf("termo com traço: %v", err)
	}
	if _, _, err := BuscarTexto(ctx, pasta, "p", false); !errors.Is(err, ErrTermoCur) {
		t.Errorf("termo curto: %v", err)
	}
	if _, _, err := BuscarTexto(ctx, t.TempDir(), "pedido", false); !errors.Is(err, ErrSemGit) {
		t.Errorf("sem git: %v", err)
	}
}

func TestTrechoCortaEmVoltaDoTermo(t *testing.T) {
	linha := strings.Repeat("x", 500) + "alvo" + strings.Repeat("y", 500)
	tr := trecho(linha, "ALVO", false)
	if !strings.Contains(tr, "alvo") || !strings.HasPrefix(tr, "…") || !strings.HasSuffix(tr, "…") || len([]rune(tr)) > MaxTrecho+2 {
		t.Errorf("trecho: %q", tr)
	}
	if trecho("\t\tcurto", "c", true) != "curto" {
		t.Error("o começo em branco deveria sair")
	}
}

func TestMudancasContraABase(t *testing.T) {
	ctx := context.Background()
	pasta := repositorioDaTarefa(t)
	m, err := ListarMudancas(ctx, pasta)
	if err != nil {
		t.Fatal(err)
	}
	if m.Base != "main" || m.Mais {
		t.Errorf("base: %+v", m)
	}
	var got []string
	for _, a := range m.Arquivos {
		got = append(got, a.Estado+" "+a.Caminho)
	}
	want := "M src/pedidos.go|A src/tela_pedidos.go|D src/velho.go|A .env|A rascunho.txt"
	if strings.Join(got, "|") != want {
		t.Errorf("mudanças:\n%s\nesperava\n%s", strings.Join(got, "|"), want)
	}
	if m.Arquivos[0].Mais != 1 || m.Arquivos[0].Menos != 1 {
		t.Errorf("contagem: %+v", m.Arquivos[0])
	}
	if _, err := ListarMudancas(ctx, t.TempDir()); !errors.Is(err, ErrSemGit) {
		t.Errorf("sem git: %v", err)
	}
}

func TestDiferencaDeUmArquivo(t *testing.T) {
	ctx := context.Background()
	pasta := repositorioDaTarefa(t)
	d, err := VerDiferenca(ctx, pasta, "src/pedidos.go", false)
	if err != nil {
		t.Fatal(err)
	}
	if d.Tipo != "diff" || d.Base != "main" {
		t.Fatalf("diff: %+v", d)
	}
	var menos, mais *LinhaDiff
	for i, l := range d.Linhas {
		switch l.Tipo {
		case "menos":
			menos = &d.Linhas[i]
		case "mais":
			mais = &d.Linhas[i]
		}
	}
	if d.Linhas[0].Tipo != "trecho" || menos == nil || mais == nil || menos.Antiga != 4 || mais.Nova != 4 || mais.Texto != "    return 2 // TotalDoPedido" {
		t.Errorf("linhas: %+v", d.Linhas)
	}
	// Novo fora do git: inteiro como "mais".
	d, err = VerDiferenca(ctx, pasta, "rascunho.txt", false)
	if err != nil || d.Tipo != "diff" || len(d.Linhas) != 3 || d.Linhas[2].Nova != 2 {
		t.Errorf("novo: %+v %v", d, err)
	}
	// Sensível: só com mostrar.
	if d, _ := VerDiferenca(ctx, pasta, ".env", false); d.Tipo != "sensivel" || len(d.Linhas) != 0 {
		t.Errorf("sensível: %+v", d)
	}
	if d, _ := VerDiferenca(ctx, pasta, ".gitignore", false); d.Tipo != "igual" {
		t.Errorf("igual: %+v", d)
	}
	for _, fora := range []string{"../fora.txt", "/etc/passwd", ".git/config"} {
		if _, err := VerDiferenca(ctx, pasta, fora, false); err == nil {
			t.Errorf("%s deveria ser recusado", fora)
		}
	}
}

func TestLerDiffNumeraAsLinhas(t *testing.T) {
	texto := "diff --git a/x b/x\nindex 1..2\n--- a/x\n+++ b/x\n@@ -10,3 +10,3 @@ func\n a\n-b\n+c\n d\n"
	linhas := lerDiff(texto, false)
	if len(linhas) != 5 || linhas[1].Antiga != 10 || linhas[1].Nova != 10 || linhas[2].Antiga != 11 || linhas[3].Nova != 11 || linhas[4].Antiga != 12 {
		t.Errorf("linhas: %+v", linhas)
	}
}
