//go:build unix

package api

import (
	"bytes"
	"image"
	"image/png"
	"io"
	"net/http"
	"os"
	"strings"
	"testing"
)

func TestAgenteRecebeEAnotaConhecimento(t *testing.T) {
	argumentos, _ := claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	eventos := ouvirComo(t, tokenTela, srv.URL, "1")

	// Sem notas, o Claude Code começa como antes.
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	esperarAte(t, "o primeiro agente começar", func() bool { return strings.Contains(lerArquivo(argumentos), "--mcp-config") })
	if strings.Contains(lerArquivo(argumentos), "append-system-prompt") {
		t.Fatal("sem notas, nada vai ao prompt")
	}

	status, nota := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/workspaces/1/conhecimento",
		map[string]any{"titulo": "Logs", "texto": "As pastas com números são os logs dos aparelhos."})
	if status != http.StatusOK || nota["estado"] != "aprovada" {
		t.Fatalf("nota: %d %v", status, nota)
	}
	eventos.esperar("a nota", func(m map[string]any) bool { return m["tipo"] == "conhecimento.mudou" && m["acao"] == "criado" })

	// O segundo agente recebe a nota num arquivo, fora da linha de comando.
	os.Remove(argumentos)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/2/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	esperarAte(t, "o segundo agente começar", func() bool { return strings.Contains(lerArquivo(argumentos), "--mcp-config") })
	args := lerArquivo(argumentos)
	if !strings.Contains(args, "--append-system-prompt-file "+servidor.DirCanal+"/agentes/2.conhecimento.md") || strings.Contains(args, "aparelhos") {
		t.Fatalf("argumentos: %s", args)
	}
	if !strings.Contains(lerArquivo(servidor.DirCanal+"/agentes/2.conhecimento.md"), "logs dos aparelhos") {
		t.Fatal("o arquivo não tem a nota")
	}

	// O que o agente anota fica como sugestão, com o selo contado.
	token := lerArquivo(servidor.DirCanal + "/agentes/2.token")
	status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/conhecimento", map[string]any{"titulo": "Testes", "texto": "Rode make teste."})
	if status != http.StatusOK || r["estado"] != "sugerida" {
		t.Fatalf("anotar: %d %v", status, r)
	}
	eventos.esperar("o selo", func(m map[string]any) bool { return m["tipo"] == "conhecimento.mudou" && m["sugestoes"] == float64(1) })
	_, lido := pedirComo(t, token, "GET", srv.URL+"/v1/agente/conhecimento", nil)
	if lista, _ := lido["notas"].([]any); len(lista) != 1 {
		t.Fatalf("o agente lê só as aprovadas: %v", lido)
	}
	if status, _ := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/agente/conhecimento", nil); status != http.StatusUnauthorized {
		t.Errorf("a tela leu pela rota do agente: %d", status)
	}
}

func TestImagemDoWorkspace(t *testing.T) {
	srv, _ := servidorComAgentes(t)
	montarTarefas(t, srv.URL)

	var bruto bytes.Buffer
	png.Encode(&bruto, image.NewRGBA(image.Rect(0, 0, 400, 300)))
	pedido, _ := http.NewRequest("PUT", srv.URL+"/v1/workspaces/1/icone", &bruto)
	pedido.Header.Set("Authorization", "Bearer "+tokenTela)
	resposta, err := http.DefaultClient.Do(pedido)
	if err != nil || resposta.StatusCode != http.StatusOK {
		t.Fatalf("enviar: %v %v", resposta, err)
	}
	resposta.Body.Close()

	pedido, _ = http.NewRequest("GET", srv.URL+"/v1/workspaces/1/icone", nil)
	pedido.Header.Set("Authorization", "Bearer "+tokenTela)
	resposta, err = http.DefaultClient.Do(pedido)
	if err != nil {
		t.Fatal(err)
	}
	corpo, _ := io.ReadAll(resposta.Body)
	resposta.Body.Close()
	img, err := png.Decode(bytes.NewReader(corpo))
	if err != nil || img.Bounds().Dx() != 128 || img.Bounds().Dy() != 128 {
		t.Fatalf("imagem gravada: %v %v", img, err)
	}

	pedido, _ = http.NewRequest("PUT", srv.URL+"/v1/workspaces/1/icone", strings.NewReader("não é imagem"))
	pedido.Header.Set("Authorization", "Bearer "+tokenTela)
	if resposta, _ = http.DefaultClient.Do(pedido); resposta.StatusCode != http.StatusBadRequest {
		t.Fatalf("aceitou o que não é imagem: %d", resposta.StatusCode)
	}

	status, ws := pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/workspaces/1", map[string]any{"nome": "Estudos"})
	if status != http.StatusOK || ws["nome"] != "Estudos" || ws["icone"] == "" {
		t.Fatalf("renomear: %d %v", status, ws)
	}
}
