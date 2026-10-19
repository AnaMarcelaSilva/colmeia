//go:build unix

package api

import (
	"fmt"
	"net/http"
	"strings"
	"testing"
)

// comListaFalada: perfil 1 com os comandos ligados, o projeto loja-web (com
// a configuração servidor) e `esperando` tarefas em "Aguardando você".
func comListaFalada(t *testing.T, esperando int) string {
	t.Helper()
	srv := servidorComVoz(t, modeloDeTeste)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "trabalho"})
	if status, p := pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "loja-web", "caminho": t.TempDir()}); status != http.StatusOK {
		t.Fatalf("projeto: %d %v", status, p)
	}
	pedir(t, "POST", srv.URL+"/v1/projetos/1/comandos", map[string]any{"nome": "servidor", "comando": "sleep 30"})
	for i := range esperando {
		_, r := pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]any{"titulo": fmt.Sprintf("Tarefa %d", i+1)})
		if status, r := pedir(t, "PATCH", fmt.Sprintf("%s/v1/tarefas/%v", srv.URL, r["id"]), map[string]any{"coluna": "aguardando"}); status != http.StatusOK {
			t.Fatalf("mover: %d %v", status, r)
		}
	}
	eventos := ouvir(t, srv.URL, "1")
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/baixar", map[string]string{"ligar": "comandos"}); status != http.StatusOK {
		t.Fatal(status)
	}
	eventos.esperar("perfil.voz", tipo("perfil.voz"))
	return srv.URL
}

func lista(t *testing.T, url string) map[string]any {
	t.Helper()
	status, r := pedir(t, "POST", url+"/v1/perfis/1/voz/lista", nil)
	if status != http.StatusOK {
		t.Fatalf("lista: %d %v", status, r)
	}
	return r
}

func naLista(t *testing.T, url, sessao, frase string) map[string]any {
	t.Helper()
	status, r := pedir(t, "POST", url+"/v1/perfis/1/voz/comando", map[string]string{"texto": frase, "lista": sessao})
	if status != http.StatusOK {
		t.Fatalf("%q: %d %v", frase, status, r)
	}
	return r
}

func linhas(r map[string]any) []string {
	var ls []string
	for _, l := range r["lista"].(map[string]any)["linhas"].([]any) {
		ls = append(ls, l.(string))
	}
	return ls
}

func TestListaFaladaEComandosComNumero(t *testing.T) {
	url := comListaFalada(t, 3)
	r := lista(t, url)
	sessao, _ := r["sessao"].(string)
	if sessao == "" || r["ouvir"] != true || !strings.Contains(r["prompt"].(string), "Abrir a um") {
		t.Fatalf("lista: %v", r)
	}
	if r["texto"] != "1. Tarefa 1: espera você. 2. Tarefa 2: espera você. 3. Tarefa 3: espera você." {
		t.Errorf("fala: %v", r["texto"])
	}
	if ls := linhas(r); len(ls) != 3 || ls[0] != "1. Tarefa 1: espera você." {
		t.Errorf("linhas: %v", ls)
	}
	// A ficha é do perfil: em outro, não vale.
	if status, r := pedir(t, "POST", url+"/v1/perfis/2/voz/comando", map[string]string{"texto": "abrir a um", "lista": sessao}); status == http.StatusOK && r["abrir"] != nil {
		t.Errorf("a lista valeu em outro perfil: %v", r)
	}

	// Mal transcrito, a abelha pergunta de novo uma vez (com a lista aberta)...
	r = naLista(t, url, sessao, "hã, é, sei lá")
	if r["texto"] != "Não entendi. Diga, por exemplo: abrir a um." || r["ouvir"] != true || r["sessao"] != sessao {
		t.Fatalf("de novo: %v", r)
	}
	// ...e entende a transcrição imperfeita.
	r = naLista(t, url, sessao, "Abriram.")
	if r["abrir"] != 1.0 || r["sessao"] != nil || r["ouvir"] != nil {
		t.Fatalf("abrir a um: %v", r)
	}
	// Feita a ação, a lista fecha.
	if r := naLista(t, url, sessao, "abrir a dois"); !strings.Contains(r["texto"].(string), "já não vale") {
		t.Errorf("lista usada: %v", r)
	}

	// Concluir pede o "sim", com o microfone aberto de novo para ele.
	sessao = lista(t, url)["sessao"].(string)
	r = naLista(t, url, sessao, "Concluir a terceira.")
	if r["confirmar"] == nil || r["ouvir"] != true || r["sessao"] != sessao || r["prompt"] != vocabularioConfirmar {
		t.Fatalf("concluir: %v", r)
	}
	if r := naLista(t, url, sessao, "Sim."); r["texto"] != "Pronto, a tarefa Tarefa 3 foi para Concluído." {
		t.Fatalf("sim: %v", r)
	}

	// Enviar para revisão acontece na hora; a lista nova já não tem as duas.
	sessao = lista(t, url)["sessao"].(string)
	if r := naLista(t, url, sessao, "Enviar a 2 para revisão."); r["texto"] != "Pronto, a tarefa Tarefa 2 foi para Revisão." {
		t.Fatalf("revisão: %v", r)
	}
	r = lista(t, url)
	if ls := linhas(r); len(ls) != 1 || ls[0] != "1. Tarefa 1: espera você." {
		t.Fatalf("lista depois: %v", ls)
	}
	sessao = r["sessao"].(string)
	// Um número que a lista não tem.
	if r := naLista(t, url, sessao, "abrir a sete"); r["texto"] != "A lista só tem o número um. Diga, por exemplo: abrir a um." {
		t.Errorf("fora: %v", r)
	}
	// Na segunda vez sem entender, a lista fecha.
	if r := naLista(t, url, sessao, "blá blá"); r["ouvir"] != nil || !strings.Contains(r["texto"].(string), "de novo") {
		t.Errorf("segunda vez: %v", r)
	}

	// Responder sem agente rodando: diz o que dá para fazer e escuta de novo, sem a lista.
	sessao = lista(t, url)["sessao"].(string)
	if r := naLista(t, url, sessao, "responder a um"); r["ouvir"] != true || r["sessao"] != nil || !strings.Contains(r["texto"].(string), "Nenhum agente") {
		t.Errorf("responder: %v", r)
	}

	// Rodar a um roda o projeto da tarefa; parar pergunta.
	sessao = lista(t, url)["sessao"].(string)
	if r := naLista(t, url, sessao, "rodar a um"); r["texto"] != "Rodando o projeto loja-web." {
		t.Fatalf("rodar: %v", r)
	}
	sessao = lista(t, url)["sessao"].(string)
	if r := naLista(t, url, sessao, "parar a um"); r["confirmar"] == nil || r["texto"] != "Parar o projeto loja-web?" {
		t.Fatalf("parar: %v", r)
	}
	if r := naLista(t, url, sessao, "não"); r["texto"] != "Tudo bem, não fiz nada." {
		t.Errorf("não: %v", r)
	}

	// Os comandos por nome continuam valendo com a lista aberta (e a fecham).
	sessao = lista(t, url)["sessao"].(string)
	if r := naLista(t, url, sessao, "abrir a tarefa Tarefa 1"); r["abrir"] == nil {
		t.Errorf("por nome: %v", r)
	}
	if r := naLista(t, url, sessao, "fechar"); !strings.Contains(r["texto"].(string), "já não vale") {
		t.Errorf("a lista ficou: %v", r)
	}

	// Fechar e só o nome (que abre a lista).
	sessao = lista(t, url)["sessao"].(string)
	if r := naLista(t, url, sessao, "Cancela."); r["texto"] != "Tudo bem." || r["ouvir"] != nil {
		t.Errorf("fechar: %v", r)
	}
	if r := falar(t, url, "Colmeia."); r["sessao"] == nil || r["lista"] == nil {
		t.Errorf("só o nome: %v", r)
	}
}

func TestListaFaladaPorPaginas(t *testing.T) {
	url := comListaFalada(t, 7)
	r := lista(t, url)
	sessao := r["sessao"].(string)
	if !strings.HasSuffix(r["texto"].(string), "Diga próxima para ouvir mais.") || len(linhas(r)) != 5 {
		t.Fatalf("página 1: %v", r)
	}
	if r := naLista(t, url, sessao, "Anterior."); r["texto"] != "Essa já é a primeira página." || r["ouvir"] != true {
		t.Errorf("anterior: %v", r)
	}
	r = naLista(t, url, sessao, "Próxima.")
	if ls := linhas(r); len(ls) != 2 || ls[0] != "1. Tarefa 6: espera você." || r["lista"].(map[string]any)["pagina"] != 2.0 {
		t.Fatalf("página 2: %v", r)
	}
	// O número vale para a página de agora.
	if r := naLista(t, url, sessao, "abrir a dois"); r["texto"] != "Abrindo a tarefa Tarefa 7." {
		t.Errorf("abrir na página 2: %v", r)
	}
	sessao = lista(t, url)["sessao"].(string)
	if r := naLista(t, url, sessao, "Repete."); len(linhas(r)) != 5 {
		t.Errorf("repetir: %v", r)
	}
}

func TestListaFaladaVazia(t *testing.T) {
	url := comListaFalada(t, 0)
	r := lista(t, url)
	if r["texto"] != "Nada espera você e nenhum agente está trabalhando." || r["ouvir"] != true || r["prompt"] != nil {
		t.Fatalf("vazia: %v", r)
	}
	// Com a lista vazia, os comandos por nome seguem.
	if r := naLista(t, url, r["sessao"].(string), "rodar a loja web"); r["texto"] != "Rodando o projeto loja-web." {
		t.Errorf("por nome: %v", r)
	}
}
