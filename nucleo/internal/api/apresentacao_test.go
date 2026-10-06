//go:build unix

package api

import (
	"bytes"
	"encoding/json"
	"image"
	"image/jpeg"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// enviar manda um corpo em bytes com o tipo dado e devolve o status e o JSON.
func enviar(t *testing.T, url, tipo string, corpo []byte) (int, map[string]any) {
	t.Helper()
	r, err := http.Post(url, tipo, bytes.NewReader(corpo))
	if err != nil {
		t.Fatal(err)
	}
	defer r.Body.Close()
	var resultado map[string]any
	json.NewDecoder(r.Body).Decode(&resultado)
	return r.StatusCode, resultado
}

// perfilComAgente cria perfil, projeto (pasta sem git), tarefa e um agente shell.
func perfilComAgente(t *testing.T, url string) {
	t.Helper()
	t.Setenv("SHELL", "/bin/sh")
	pedir(t, "POST", url+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	pedir(t, "POST", url+"/v1/perfis/1/workspaces", map[string]string{"nome": "W"})
	pedir(t, "POST", url+"/v1/workspaces/1/projetos", map[string]string{"nome": "cliente-x", "caminho": t.TempDir()})
	pedir(t, "POST", url+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Analisar relatório"})
	if status, a := pedir(t, "POST", url+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "shell", "papel": "dev"}); status != http.StatusOK {
		t.Fatalf("criar agente: %d %v", status, a)
	}
}

func TestRotasDoHistoricoDeMensagens(t *testing.T) {
	srv := servidorComDados(t)
	perfilComAgente(t, srv.URL)

	if status, r := pedir(t, "POST", srv.URL+"/v1/agentes/1/mensagens", map[string]string{"texto": "roda os testes"}); status != http.StatusOK || r["guardada"] != true {
		t.Errorf("guardar: %d %v", status, r)
	}
	if status, r := pedir(t, "POST", srv.URL+"/v1/agentes/1/mensagens", map[string]string{"texto": "senha: hunter2222"}); status != http.StatusOK || r["guardada"] != false {
		t.Errorf("segredo: %d %v", status, r)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/agentes/1/mensagens", map[string]any{"texto": "x", "extra": 1}); status != http.StatusBadRequest {
		t.Errorf("campo desconhecido: %d", status)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/agentes/1/mensagens", map[string]any{"texto": ""}); status != http.StatusBadRequest {
		t.Errorf("texto vazio: %d", status)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/agentes/99/mensagens", map[string]string{"texto": "oi"}); status != http.StatusNotFound {
		t.Errorf("agente que não existe: %d", status)
	}
	status, lista := pedirLista(t, srv.URL+"/v1/agentes/1/mensagens")
	if status != http.StatusOK || len(lista) != 1 || lista[0]["texto"] != "roda os testes" {
		t.Errorf("listar: %d %v", status, lista)
	}
	if status, _ := pedirLista(t, srv.URL+"/v1/agentes/99/mensagens"); status != http.StatusNotFound {
		t.Errorf("listar de agente que não existe: %d", status)
	}
	if status, r := pedir(t, "DELETE", srv.URL+"/v1/agentes/1/mensagens", nil); status != http.StatusOK || r["apagadas"] != float64(1) {
		t.Errorf("limpar: %d %v", status, r)
	}
	if _, lista := pedirLista(t, srv.URL+"/v1/agentes/1/mensagens"); len(lista) != 0 {
		t.Errorf("sobrou depois de limpar: %v", lista)
	}
}

func TestNotasEApresentacao(t *testing.T) {
	srv := servidorComDados(t)
	perfilComAgente(t, srv.URL)
	hoje := time.Now().Format("2006-01-02")

	if status, n := pedir(t, "PUT", srv.URL+"/v1/tarefas/1/notas", map[string]string{"tipo": "daily", "periodo": hoje, "texto": "Mostrar o relatório"}); status != http.StatusOK || n["texto"] != "Mostrar o relatório" {
		t.Errorf("nota: %d %v", status, n)
	}
	for _, ruim := range []map[string]any{
		{"tipo": "daily", "periodo": "ontem", "texto": "x"},
		{"tipo": "mensal", "periodo": hoje, "texto": "x"},
		{"tipo": "daily", "periodo": hoje, "texto": "x", "extra": true},
	} {
		if status, _ := pedir(t, "PUT", srv.URL+"/v1/tarefas/1/notas", ruim); status != http.StatusBadRequest {
			t.Errorf("nota inválida %v: %d", ruim, status)
		}
	}
	if status, _ := pedir(t, "PUT", srv.URL+"/v1/tarefas/99/notas", map[string]string{"tipo": "daily", "periodo": hoje, "texto": "x"}); status != http.StatusNotFound {
		t.Errorf("nota de tarefa que não existe: %d", status)
	}

	// O agente rodando deixa a tarefa em andamento: ela vira slide, com a nota.
	status, deck := pedir(t, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=daily", nil)
	if status != http.StatusOK || deck["chave_nota"] != hoje {
		t.Fatalf("deck: %d %v", status, deck)
	}
	slides, _ := deck["slides"].([]any)
	if len(slides) != 1 {
		t.Fatalf("slides: %v", deck["slides"])
	}
	if s := slides[0].(map[string]any); s["nota"] != "Mostrar o relatório" || s["titulo"] != "Analisar relatório" {
		t.Errorf("slide: %v", s)
	}
	if status, _ := pedir(t, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=sprint&ultimos=7", nil); status != http.StatusOK {
		t.Errorf("sprint: %d", status)
	}
	for _, consulta := range []string{"tipo=semana", "tipo=sprint", "tipo=sprint&ultimos=0", "tipo=daily&projeto=abc"} {
		if status, _ := pedir(t, "GET", srv.URL+"/v1/perfis/1/apresentacao?"+consulta, nil); status != http.StatusBadRequest {
			t.Errorf("%s: %d", consulta, status)
		}
	}
	if status, _ := pedir(t, "GET", srv.URL+"/v1/perfis/9/apresentacao?tipo=daily", nil); status != http.StatusNotFound {
		t.Errorf("perfil que não existe: %d", status)
	}
}

func TestFotoEVideoNaTarefa(t *testing.T) {
	srv := servidorComDados(t)
	perfilComAgente(t, srv.URL)

	img := image.NewRGBA(image.Rect(0, 0, 40, 30))
	var foto bytes.Buffer
	jpeg.Encode(&foto, img, nil)
	status, r := enviar(t, srv.URL+"/v1/tarefas/1/anexos?origem=arquivo&nome=foto.jpg", "image/jpeg", foto.Bytes())
	if status != http.StatusOK || !strings.HasSuffix(r["caminho"].(string), ".png") {
		t.Fatalf("foto: %d %v", status, r)
	}
	if status, _ := enviar(t, srv.URL+"/v1/tarefas/1/anexos?origem=arquivo", "image/gif", foto.Bytes()); status != http.StatusBadRequest {
		t.Errorf("gif: %d", status)
	}

	mp4 := make([]byte, 64<<10)
	copy(mp4, []byte{0, 0, 0, 0x18, 'f', 't', 'y', 'p', 'm', 'p', '4', '2'})
	status, r = enviar(t, srv.URL+"/v1/tarefas/1/videos?nome=demo.mp4", "video/mp4", mp4)
	if status != http.StatusOK {
		t.Fatalf("vídeo: %d %v", status, r)
	}
	caminho := r["caminho"].(string)
	info, err := os.Stat(caminho)
	if err != nil || info.Mode().Perm() != 0o600 || info.Size() != int64(len(mp4)) || filepath.Ext(caminho) != ".mp4" {
		t.Errorf("arquivo do vídeo: %v %v", info, err)
	}
	pasta, _ := os.Stat(filepath.Dir(caminho))
	if pasta.Mode().Perm() != 0o700 {
		t.Errorf("pasta dos anexos com %o", pasta.Mode().Perm())
	}
	id := int(r["id"].(float64))
	status, dadosVideo := pedir(t, "GET", srv.URL+"/v1/anexos/"+itoa(id)+"/info", nil)
	if status != http.StatusOK || dadosVideo["tipo"] != "video" || dadosVideo["nome"] != "demo.mp4" || dadosVideo["caminho"] != caminho {
		t.Errorf("info do vídeo: %d %v", status, dadosVideo)
	}
	resposta, _ := http.Get(srv.URL + "/v1/anexos/" + itoa(id))
	io.Copy(io.Discard, resposta.Body)
	resposta.Body.Close()
	if resposta.StatusCode != http.StatusBadRequest {
		t.Errorf("ler vídeo como imagem: %d", resposta.StatusCode)
	}

	for _, c := range []struct {
		url, tipo string
		corpo     []byte
	}{
		{"/v1/tarefas/1/videos", "video/webm", mp4},                  // os bytes são de mp4
		{"/v1/tarefas/1/videos", "application/x-desktop", mp4},       // tipo fora da lista
		{"/v1/tarefas/1/videos", "video/mp4", []byte("#!/bin/sh\n")}, // não é vídeo
		{"/v1/tarefas/1/videos?nome=../../x.mp4", "video/mp4", mp4},  // nome com pasta
	} {
		if status, r := enviar(t, srv.URL+c.url, c.tipo, c.corpo); status != http.StatusBadRequest {
			t.Errorf("%s %s: %d %v", c.url, c.tipo, status, r)
		}
	}
	if status, _ := enviar(t, srv.URL+"/v1/tarefas/99/videos", "video/mp4", mp4); status != http.StatusNotFound {
		t.Errorf("tarefa que não existe: %d", status)
	}
	sobras, _ := filepath.Glob(filepath.Join(filepath.Dir(caminho), ".video-*"))
	if len(sobras) > 0 {
		t.Errorf("temporários sobraram: %v", sobras)
	}

	// O deck traz a foto e o vídeo do slide, do mais novo ao mais antigo.
	_, deck := pedir(t, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=daily", nil)
	slide := deck["slides"].([]any)[0].(map[string]any)
	anexos := slide["anexos"].([]any)
	if len(anexos) != 2 || anexos[0].(map[string]any)["tipo"] != "video" {
		t.Errorf("anexos do slide: %v", anexos)
	}
}

func TestTirarDaDaily(t *testing.T) {
	srv := servidorComDados(t)
	perfilComAgente(t, srv.URL)
	hoje := time.Now().Format("2006-01-02")
	url := srv.URL + "/v1/tarefas/1/daily"
	slides := func(consulta string) (int, map[string]any) {
		t.Helper()
		_, deck := pedir(t, "GET", srv.URL+"/v1/perfis/1/apresentacao?"+consulta, nil)
		s, _ := deck["slides"].([]any)
		return len(s), deck
	}
	if n, _ := slides("tipo=daily"); n != 1 {
		t.Fatalf("antes de tirar: %d slides", n)
	}

	if status, _ := pedir(t, "PUT", url, map[string]any{"dia": hoje, "fora": true}); status != http.StatusNoContent {
		t.Fatalf("tirar: %d", status)
	}
	n, deck := slides("tipo=daily")
	fora, _ := deck["fora"].([]any)
	if n != 0 || len(fora) != 1 || fora[0].(map[string]any)["titulo"] != "Analisar relatório" {
		t.Errorf("deck depois de tirar: %d slides, fora %v", n, deck["fora"])
	}
	if _, d := pedir(t, "GET", srv.URL+"/v1/perfis/1/resumo?tipo=daily", nil); strings.Contains(d["texto"].(string), "Analisar relatório") {
		t.Errorf("o texto da daily ainda cita a tarefa: %v", d["texto"])
	}
	// A sprint continua com ela.
	if n, _ := slides("tipo=sprint&ultimos=7"); n != 1 {
		t.Errorf("sprint sem a tarefa: %d slides", n)
	}

	for _, ruim := range []map[string]any{
		{"dia": "ontem", "fora": true},
		{"dia": hoje},
		{"dia": hoje, "fora": true, "extra": 1},
	} {
		if status, _ := pedir(t, "PUT", url, ruim); status != http.StatusBadRequest {
			t.Errorf("pedido inválido %v: %d", ruim, status)
		}
	}
	if status, _ := pedir(t, "PUT", srv.URL+"/v1/tarefas/99/daily", map[string]any{"dia": hoje, "fora": true}); status != http.StatusNotFound {
		t.Errorf("tarefa que não existe: %d", status)
	}

	if status, _ := pedir(t, "PUT", url, map[string]any{"dia": hoje, "fora": false}); status != http.StatusNoContent {
		t.Fatalf("trazer de volta: %d", status)
	}
	if n, deck := slides("tipo=daily"); n != 1 || deck["fora"] != nil {
		t.Errorf("depois de trazer de volta: %d slides, fora %v", n, deck["fora"])
	}
}
