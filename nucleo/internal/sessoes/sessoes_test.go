package sessoes

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestPastaDoProjetoTrocaTudoQueNaoEAlfanumerico(t *testing.T) {
	got := PastaDoProjeto("/c", "/home/ana/Área de Trabalho/meu_proj.v2")
	want := filepath.Join("/c", "projects", "-home-ana--rea-de-Trabalho-meu-proj-v2")
	if got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestListarAchaTituloEIgnoraArquivosEstranhos(t *testing.T) {
	config := t.TempDir()
	dir := "/tmp/um projeto"
	pasta := PastaDoProjeto(config, dir)
	os.MkdirAll(pasta, 0o700)
	id := "81701644-dffb-4e29-9873-ea49ec35ec50"
	conteudo := `{"type":"user","message":{"content":"` + strings.Repeat("x", 10000) + `"}}
{"type":"ai-title","aiTitle":"Tela de ambientes"}
{"type":"last-prompt","lastPrompt":"commita\ne faz push"}
`
	os.WriteFile(filepath.Join(pasta, id+".jsonl"), []byte(conteudo), 0o600)
	outro := "c2141931-f003-46e9-af83-14b284a581b0"
	os.WriteFile(filepath.Join(pasta, outro+".jsonl"), []byte(`{"type":"last-prompt","lastPrompt":"commita\ne faz push"}`+"\n"), 0o600)
	os.WriteFile(filepath.Join(pasta, "--resume.jsonl"), []byte("x"), 0o600)
	os.WriteFile(filepath.Join(pasta, "notas.txt"), []byte("x"), 0o600)

	lista, err := Listar(config, dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(lista) != 2 {
		t.Fatalf("esperava 2 conversas, veio %+v", lista)
	}
	titulos := map[string]string{}
	for _, s := range lista {
		titulos[s.ID] = s.Titulo
	}
	if titulos[id] != "Tela de ambientes" || titulos[outro] != "commita e faz push" {
		t.Fatalf("títulos errados: %v", titulos)
	}
}

func TestListarPastaSemConversas(t *testing.T) {
	lista, err := Listar(t.TempDir(), "/nao/existe")
	if err != nil || len(lista) != 0 {
		t.Fatalf("esperava lista vazia, veio %v %v", lista, err)
	}
}

func TestIDValidoRecusaOpcoes(t *testing.T) {
	for _, id := range []string{"--dangerously-skip-permissions", "", "81701644-dffb-4e29-9873-ea49ec35ec5", "81701644-DFFB-4e29-9873-ea49ec35ec50"} {
		if IDValido(id) {
			t.Errorf("%q não deveria valer", id)
		}
	}
}
