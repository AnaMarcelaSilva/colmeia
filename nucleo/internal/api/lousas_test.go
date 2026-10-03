//go:build unix

package api

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"
)

// pedirBruto manda um corpo cru (para JSON inválido ou grande demais).
func pedirBruto(t *testing.T, url string, corpo []byte) int {
	t.Helper()
	r, err := http.Post(url, "application/json", bytes.NewReader(corpo))
	if err != nil {
		t.Fatal(err)
	}
	io.Copy(io.Discard, r.Body)
	r.Body.Close()
	return r.StatusCode
}

func nota(x float64, texto string) map[string]any {
	return map[string]any{"tipo": "nota", "x": x, "y": 0, "largura": 240, "altura": 120, "texto": texto}
}

func TestLousaPelaTela(t *testing.T) {
	srv := servidorComDados(t)
	perfilComAgente(t, srv.URL)
	eventos := ouvir(t, srv.URL, "1")

	status, aberta := pedir(t, "POST", srv.URL+"/v1/workspaces/1/lousa", nil)
	lousa, _ := aberta["lousa"].(map[string]any)
	if status != http.StatusOK || lousa["id"] != 1.0 || lousa["dono"].(map[string]any)["workspace_id"] != 1.0 {
		t.Fatalf("abrir a lousa do workspace: %d %v", status, aberta)
	}
	seqRetrato := aberta["seq"].(float64)
	if status, r := pedir(t, "POST", srv.URL+"/v1/tarefas/1/lousa", nil); status != http.StatusOK || r["lousa"].(map[string]any)["id"] != 2.0 {
		t.Errorf("abrir a lousa da tarefa: %d %v", status, r)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/workspaces/9/lousa", nil); status != http.StatusNotFound {
		t.Errorf("workspace que não existe: %d", status)
	}

	// Criar com ref e uma ligação no mesmo lote; o aviso chega às telas.
	status, r := pedir(t, "POST", srv.URL+"/v1/lousas/1/operacoes", map[string]any{"operacoes": []any{
		map[string]any{"op": "criar", "ref": "a", "elemento": nota(0, "# Tela")},
		map[string]any{"op": "criar", "ref": "b", "elemento": nota(400, "API")},
		map[string]any{"op": "criar", "ref": "l", "elemento": map[string]any{"tipo": "ligacao", "de": "a", "para": "b", "texto": "chama"}},
	}})
	if status != http.StatusOK || len(r["elementos"].([]any)) != 3 || r["refs"].(map[string]any)["l"] == nil {
		t.Fatalf("criar: %d %v", status, r)
	}
	aviso := eventos.esperar("lousa.mudou", func(m map[string]any) bool { return m["tipo"] == "lousa.mudou" })
	if aviso["seq"].(float64) <= seqRetrato || len(aviso["elementos"].([]any)) != 3 || aviso["dono"].(map[string]any)["workspace_id"] != 1.0 {
		t.Errorf("aviso da lousa: %v", aviso)
	}
	if _, temAgente := aviso["agente_id"]; temAgente {
		t.Error("a mudança da tela saiu como do agente")
	}
	a := int(r["refs"].(map[string]any)["a"].(float64))

	// 409: versão velha, nada gravado, com o estado atual.
	pedir(t, "POST", srv.URL+"/v1/lousas/1/operacoes", map[string]any{"operacoes": []any{
		map[string]any{"op": "alterar", "id": a, "versao": 1, "campos": map[string]any{"x": 50}},
	}})
	status, r = pedir(t, "POST", srv.URL+"/v1/lousas/1/operacoes", map[string]any{"operacoes": []any{
		map[string]any{"op": "alterar", "id": a, "versao": 1, "campos": map[string]any{"cor": "azul"}},
		map[string]any{"op": "remover", "id": 999, "versao": 1},
	}})
	if status != http.StatusConflict || r["erro"] == nil || len(r["elementos"].([]any)) != 1 || len(r["removidos"].([]any)) != 1 {
		t.Fatalf("conflito: %d %v", status, r)
	}
	if e := r["elementos"].([]any)[0].(map[string]any); e["versao"] != 2.0 || e["x"] != 50.0 || e["cor"] != "amarelo" {
		t.Errorf("estado atual no conflito: %v", e)
	}
	// Remover leva a ligação junto.
	status, r = pedir(t, "POST", srv.URL+"/v1/lousas/1/operacoes", map[string]any{"operacoes": []any{map[string]any{"op": "remover", "id": a, "versao": 2}}})
	if status != http.StatusOK || len(r["removidos"].([]any)) != 2 {
		t.Errorf("remover: %d %v", status, r)
	}
	if _, r := pedir(t, "GET", srv.URL+"/v1/lousas/1", nil); len(r["elementos"].([]any)) != 1 {
		t.Errorf("recarregar: %v", r)
	}

	// JSON estrito: campo desconhecido, tipo errado e corpo grande demais.
	for _, corpo := range []string{
		`{"operacoes":[{"op":"criar","elemento":{"tipo":"nota","x":0,"y":0,"largura":100,"altura":100,"forma":"losango"}}]}`,
		`{"operacoes":[{"op":"alterar","id":2,"versao":1,"campos":{"x":"longe"}}]}`,
		`{"operacoes":[],"extra":1}`,
		`{"operacoes":[{"op":"criar","elemento":{"tipo":"nota","x":0,"y":0,"largura":100,"altura":100}}]} {}`,
	} {
		if status := pedirBruto(t, srv.URL+"/v1/lousas/1/operacoes", []byte(corpo)); status != http.StatusBadRequest {
			t.Errorf("corpo %s: %d", corpo, status)
		}
	}
	grande := `{"operacoes":[{"op":"criar","elemento":{"tipo":"nota","x":0,"y":0,"largura":100,"altura":100,"texto":"` + strings.Repeat("a", limiteLoteLousa) + `"}}]}`
	if status := pedirBruto(t, srv.URL+"/v1/lousas/1/operacoes", []byte(grande)); status != http.StatusBadRequest {
		t.Errorf("corpo grande demais: %d", status)
	}
	// Mas um lote acima dos 64 KB gerais passa.
	medio, _ := json.Marshal(map[string]any{"operacoes": []any{map[string]any{"op": "criar", "elemento": nota(0, strings.Repeat("b", 7000))},
		map[string]any{"op": "criar", "elemento": nota(0, strings.Repeat("c", 7000))}, map[string]any{"op": "criar", "elemento": nota(0, strings.Repeat("d", 7000))},
		map[string]any{"op": "criar", "elemento": nota(0, strings.Repeat("e", 7000))}, map[string]any{"op": "criar", "elemento": nota(0, strings.Repeat("f", 7000))},
		map[string]any{"op": "criar", "elemento": nota(0, strings.Repeat("g", 7000))}, map[string]any{"op": "criar", "elemento": nota(0, strings.Repeat("h", 7000))},
		map[string]any{"op": "criar", "elemento": nota(0, strings.Repeat("i", 7000))}, map[string]any{"op": "criar", "elemento": nota(0, strings.Repeat("j", 7000))},
		map[string]any{"op": "criar", "elemento": nota(0, strings.Repeat("k", 7000))}}})
	if len(medio) <= limiteCorpo {
		t.Fatal("o lote de teste deveria passar de 64 KB")
	}
	if status := pedirBruto(t, srv.URL+"/v1/lousas/1/operacoes", medio); status != http.StatusOK {
		t.Errorf("lote de %d bytes: %d", len(medio), status)
	}
	// Segredo: recusado sem repetir o texto.
	status, r = pedir(t, "POST", srv.URL+"/v1/lousas/1/operacoes", map[string]any{"operacoes": []any{map[string]any{"op": "criar", "elemento": nota(0, "senha: hunter2222")}}})
	if status != http.StatusBadRequest || strings.Contains(r["erro"].(string), "hunter") {
		t.Errorf("segredo: %d %v", status, r)
	}
	// Nada disso entrou na corrente de eventos.
	_, linha := pedir(t, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo", nil)
	if bruto, _ := json.Marshal(linha); strings.Contains(string(bruto), "lousa") {
		t.Errorf("mexer na lousa apareceu na linha do tempo: %s", bruto)
	}
}

func TestImagemEVideoDaLousaFicamForaDaLinhaEDoSlide(t *testing.T) {
	srv := servidorComDados(t)
	perfilComAgente(t, srv.URL)
	status, img := enviar(t, srv.URL+"/v1/perfis/1/anexos?origem=colagem&lousa=1", "image/png", pngPequeno(t))
	if status != http.StatusOK {
		t.Fatalf("imagem da lousa: %d %v", status, img)
	}
	mp4 := make([]byte, 64<<10)
	copy(mp4, []byte{0, 0, 0, 0x18, 'f', 't', 'y', 'p', 'm', 'p', '4', '2'})
	status, video := enviar(t, srv.URL+"/v1/perfis/1/videos?nome=demo.mp4&lousa=1", "video/mp4", mp4)
	if status != http.StatusOK {
		t.Fatalf("vídeo do perfil: %d %v", status, video)
	}
	if status, _ := enviar(t, srv.URL+"/v1/tarefas/1/anexos?origem=colagem&lousa=1", "image/png", pngPequeno(t)); status != http.StatusBadRequest {
		t.Errorf("anexo da lousa na tarefa: %d", status)
	}
	if status, _ := enviar(t, srv.URL+"/v1/perfis/1/anexos?origem=colagem&lousa=sim", "image/png", pngPequeno(t)); status != http.StatusBadRequest {
		t.Errorf("lousa=sim: %d", status)
	}
	// Na lousa da tarefa, com o tamanho do anexo para a tela.
	pedir(t, "POST", srv.URL+"/v1/tarefas/1/lousa", nil)
	status, r := pedir(t, "POST", srv.URL+"/v1/lousas/1/operacoes", map[string]any{"operacoes": []any{
		map[string]any{"op": "criar", "elemento": map[string]any{"tipo": "imagem", "x": 0, "y": 0, "largura": 400, "altura": 300, "anexo_id": img["id"]}},
		map[string]any{"op": "criar", "elemento": map[string]any{"tipo": "video", "x": 0, "y": 400, "largura": 320, "altura": 180, "anexo_id": video["id"]}},
	}})
	if status != http.StatusOK {
		t.Fatalf("imagem e vídeo na lousa: %d %v", status, r)
	}
	if v := r["elementos"].([]any)[1].(map[string]any)["anexo"].(map[string]any); v["nome"] != "demo.mp4" || v["bytes"] != float64(len(mp4)) {
		t.Errorf("informação do vídeo: %v", v)
	}
	_, linha := pedir(t, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo", nil)
	if bruto, _ := json.Marshal(linha); strings.Contains(string(bruto), "anexad") {
		t.Errorf("anexo da lousa na linha do tempo: %s", bruto)
	}
	// O slide da tarefa leva o resumo da lousa, e não os anexos dela.
	_, deck := pedir(t, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=sprint&ultimos=7", nil)
	slides, _ := deck["slides"].([]any)
	if len(slides) != 1 {
		t.Fatalf("slides: %v", deck)
	}
	slide := slides[0].(map[string]any)
	if anexos, _ := slide["anexos"].([]any); len(anexos) != 0 {
		t.Errorf("anexos da lousa no slide: %v", anexos)
	}
	if l, _ := slide["lousa"].(map[string]any); l["id"] != 1.0 || l["elementos"] != 2.0 {
		t.Errorf("lousa no slide: %v", slide["lousa"])
	}
}

func TestAgenteNaLousaDaTarefa(t *testing.T) {
	claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/2/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	dir := filepath.Join(servidor.DirCanal, "agentes")
	esperarAte(t, "o token do agente", func() bool { return len(lerArquivo(filepath.Join(dir, "1.token"))) == 64 })
	token := lerArquivo(filepath.Join(dir, "1.token"))

	// Algo da tela na lousa da tarefa 1 e na da tarefa 2.
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/lousa", nil)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/2/lousa", nil)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/lousas/1/operacoes", map[string]any{"operacoes": []any{map[string]any{"op": "criar", "elemento": nota(0, "da tela")}}})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/lousas/2/operacoes", map[string]any{"operacoes": []any{map[string]any{"op": "criar", "elemento": nota(0, "SEGREDO-DA-TAREFA-2")}}})

	// A tela ouve os eventos com o token dela.
	ctx, cancelar := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancelar()
	conn, _, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(srv.URL, "http")+"/v1/perfis/1/eventos",
		&websocket.DialOptions{HTTPHeader: http.Header{"Authorization": {"Bearer " + tokenTela}}})
	if err != nil {
		t.Fatal(err)
	}
	defer conn.CloseNow()
	tela := &ouvinte{t, conn, ctx}
	tela.proxima()

	const marcador = "MARCADOR-DO-AGENTE-91c"
	status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/lousa/elementos", map[string]any{"elementos": []any{
		map[string]any{"ref": "tela", "tipo": "nota", "texto": "# Tela\n" + marcador},
		map[string]any{"ref": "api", "tipo": "codigo", "texto": "tela -> api"},
		map[string]any{"tipo": "ligacao", "de": "tela", "para": "api"},
	}})
	if status != http.StatusOK || len(r["ids"].([]any)) != 3 || r["lousa"] != 1.0 {
		t.Fatalf("acrescentar: %d %v", status, r)
	}
	// A tela recebe os itens inteiros, marcados como do agente.
	aviso := tela.esperar("lousa.mudou do agente", func(m map[string]any) bool { return m["tipo"] == "lousa.mudou" && m["agente_id"] != nil })
	if len(aviso["elementos"].([]any)) != 3 || aviso["dono"].(map[string]any)["tarefa_id"] != 1.0 || aviso["evento"] == nil {
		t.Errorf("aviso do agente: %v", aviso)
	}
	if e := aviso["elementos"].([]any)[0].(map[string]any); e["autor"] != "agente" || !strings.Contains(e["texto"].(string), marcador) {
		t.Errorf("item do agente no aviso: %v", e)
	}
	// Lê só a lousa da própria tarefa.
	status, r = pedirComo(t, token, "GET", srv.URL+"/v1/agente/lousa", nil)
	bruto, _ := json.Marshal(r)
	if status != http.StatusOK || len(r["elementos"].([]any)) != 4 || strings.Contains(string(bruto), "SEGREDO-DA-TAREFA-2") {
		t.Errorf("ler a lousa: %d %s", status, bruto)
	}
	// Não alcança as rotas da tela (alterar, remover, outra lousa).
	for _, rota := range []string{"/v1/lousas/2/operacoes", "/v1/tarefas/2/lousa", "/v1/lousas/1/operacoes"} {
		if status, _ := pedirComo(t, token, "POST", srv.URL+rota, map[string]any{"operacoes": []any{}}); status != http.StatusUnauthorized {
			t.Errorf("token do agente em %s: %d", rota, status)
		}
	}
	// Campo desconhecido (tentar escolher a tarefa) é recusado.
	if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/lousa/elementos", map[string]any{"elementos": []any{nota(0, "x")}, "tarefa": 2}); status != http.StatusBadRequest {
		t.Errorf("campo tarefa aceito: %d", status)
	}
	// O evento entra na linha do tempo, sem o texto.
	_, linha := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo", nil)
	bruto, _ = json.Marshal(linha)
	if !strings.Contains(string(bruto), "Claude Code (dev) acrescentou 3 itens à lousa") || strings.Contains(string(bruto), marcador) {
		t.Errorf("linha do tempo: %s", bruto)
	}
}
