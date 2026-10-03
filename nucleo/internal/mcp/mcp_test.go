package mcp

import (
	"bufio"
	"context"
	"encoding/json"
	"io"
	"strings"
	"sync"
	"testing"
)

// nucleoFalso guarda os pedidos e responde como o núcleo.
type nucleoFalso struct {
	mu      sync.Mutex
	pedidos []string
	corpos  []any
}

func (n *nucleoFalso) Pedir(_ context.Context, metodo, caminho string, corpo any) (int, []byte, error) {
	n.mu.Lock()
	n.pedidos = append(n.pedidos, metodo+" "+caminho)
	n.corpos = append(n.corpos, corpo)
	n.mu.Unlock()
	switch {
	case caminho == "/v1/agente/tarefa":
		return 200, []byte(`{"titulo":"Nova tela de pedidos","pedidos":[]}`), nil
	case caminho == "/v1/agente/nota" && metodo == "PUT":
		return 200, []byte(`{"tipo":"daily","periodo":"2026-10-02","texto":"x"}`), nil
	case strings.HasPrefix(caminho, "/v1/agente/navegador/captura"):
		return 200, []byte(`{"png":"iVBORw0KGgo=","anexo":9}`), nil
	case caminho == "/v1/agente/navegador":
		return 400, []byte(`{"erro":"Só endereços http, https ou arquivos desta pasta"}`), nil
	case caminho == "/v1/agente/lousa":
		return 200, []byte(`{"lousa":3,"elementos":[{"id":1,"tipo":"nota","texto":"Fluxo da tela"}]}`), nil
	case caminho == "/v1/agente/lousa/elementos":
		return 200, []byte(`{"lousa":3,"ids":[7,8,9],"refs":{"a":7,"b":8}}`), nil
	}
	return 404, []byte(`{"erro":"não encontrado"}`), nil
}

type clienteMCP struct {
	t     *testing.T
	envia io.WriteCloser
	le    *bufio.Scanner
	fim   chan error
}

func conectar(t *testing.T, n Cliente) *clienteMCP {
	t.Helper()
	entradaLe, entradaEscreve := io.Pipe()
	saidaLe, saidaEscreve := io.Pipe()
	s := &Servidor{Nucleo: n, Versao: "teste"}
	c := &clienteMCP{t: t, envia: entradaEscreve, le: bufio.NewScanner(saidaLe), fim: make(chan error, 1)}
	c.le.Buffer(nil, 4<<20)
	go func() {
		c.fim <- s.Servir(context.Background(), entradaLe, saidaEscreve)
		saidaEscreve.Close()
	}()
	t.Cleanup(func() { entradaEscreve.Close() })
	return c
}

func (c *clienteMCP) mandar(linha string) {
	c.t.Helper()
	if _, err := io.WriteString(c.envia, linha+"\n"); err != nil {
		c.t.Fatal(err)
	}
}

func (c *clienteMCP) receber() map[string]any {
	c.t.Helper()
	if !c.le.Scan() {
		c.t.Fatalf("sem resposta: %v", c.le.Err())
	}
	var m map[string]any
	if err := json.Unmarshal(c.le.Bytes(), &m); err != nil {
		c.t.Fatalf("resposta inválida %q: %v", c.le.Text(), err)
	}
	return m
}

func (c *clienteMCP) chamar(id int, ferramenta, argumentos string) map[string]any {
	c.t.Helper()
	c.mandar(`{"jsonrpc":"2.0","id":` + itoa(id) + `,"method":"tools/call","params":{"name":"` + ferramenta + `","arguments":` + argumentos + `,"_meta":{"progressToken":1}}}`)
	return c.receber()
}

func itoa(n int) string { b, _ := json.Marshal(n); return string(b) }

func resultado(t *testing.T, m map[string]any) (string, bool) {
	t.Helper()
	r, ok := m["result"].(map[string]any)
	if !ok {
		t.Fatalf("sem resultado: %v", m)
	}
	conteudo, _ := r["content"].([]any)
	var textos []string
	for _, c := range conteudo {
		if c, _ := c.(map[string]any); c["type"] == "text" {
			textos = append(textos, c["text"].(string))
		}
	}
	erro, _ := r["isError"].(bool)
	return strings.Join(textos, "\n"), erro
}

func TestServidorMCP(t *testing.T) {
	nucleo := &nucleoFalso{}
	c := conectar(t, nucleo)

	c.mandar(`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"falso","version":"1"}}}`)
	ini := c.receber()["result"].(map[string]any)
	if ini["protocolVersion"] != "2025-03-26" || ini["serverInfo"].(map[string]any)["name"] != "colmeia" {
		t.Errorf("initialize: %v", ini)
	}
	c.mandar(`{"jsonrpc":"2.0","method":"notifications/initialized"}`)
	// Versão desconhecida: responde com a mais nova.
	c.mandar(`{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}`)
	if v := c.receber()["result"].(map[string]any)["protocolVersion"]; v != Versoes[0] {
		t.Errorf("versão desconhecida: %v", v)
	}

	c.mandar(`{"jsonrpc":"2.0","id":3,"method":"tools/list"}`)
	ferramentas := c.receber()["result"].(map[string]any)["tools"].([]any)
	var nomes []string
	for _, f := range ferramentas {
		nomes = append(nomes, f.(map[string]any)["name"].(string))
	}
	if strings.Join(nomes, ",") != "ler_tarefa,ler_nota,complementar_nota,escrever_nota,anexar_imagem,abrir_navegador,capturar_navegador,concluir_pedido,ler_lousa,acrescentar_a_lousa" {
		t.Errorf("ferramentas: %v", nomes)
	}

	if texto, erro := resultado(t, c.chamar(4, "ler_tarefa", `{}`)); erro || !strings.Contains(texto, "Nova tela de pedidos") {
		t.Errorf("ler_tarefa: %q %v", texto, erro)
	}
	if texto, erro := resultado(t, c.chamar(5, "complementar_nota", `{"texto":"Total de testes: 42","pedido":7}`)); erro || !strings.Contains(texto, "daily") {
		t.Errorf("complementar_nota: %q %v", texto, erro)
	}
	// Argumento desconhecido: erro da ferramenta, sem chegar ao núcleo.
	antes := len(nucleo.pedidos)
	if texto, erro := resultado(t, c.chamar(6, "complementar_nota", `{"texto":"x","tarefa":3}`)); !erro || !strings.Contains(texto, "argumentos inválidos") {
		t.Errorf("argumento desconhecido: %q %v", texto, erro)
	}
	if texto, erro := resultado(t, c.chamar(7, "complementar_nota", `{"texto":5}`)); !erro {
		t.Errorf("argumento com tipo errado: %q", texto)
	}
	if len(nucleo.pedidos) != antes {
		t.Errorf("argumentos inválidos chegaram ao núcleo: %v", nucleo.pedidos)
	}
	// Erro do núcleo volta como isError, com a mensagem dele.
	if texto, erro := resultado(t, c.chamar(8, "abrir_navegador", `{"url":"javascript:alert(1)"}`)); !erro || !strings.Contains(texto, "Só endereços") {
		t.Errorf("url recusada: %q %v", texto, erro)
	}
	m := c.chamar(9, "capturar_navegador", `{}`)
	conteudo := m["result"].(map[string]any)["content"].([]any)
	if imagem := conteudo[0].(map[string]any); imagem["type"] != "image" || imagem["mimeType"] != "image/png" || imagem["data"] == "" {
		t.Errorf("captura: %v", conteudo)
	}
	if corpo := nucleo.corpos[len(nucleo.corpos)-1].(map[string]any); corpo["anexar"] != true {
		t.Errorf("captura anexa por padrão: %v", corpo)
	}

	// A lousa da tarefa: ler e acrescentar (com refs e ligações).
	if texto, erro := resultado(t, c.chamar(20, "ler_lousa", `{}`)); erro || !strings.Contains(texto, "Fluxo da tela") {
		t.Errorf("ler_lousa: %q %v", texto, erro)
	}
	itens := `{"itens":[{"ref":"a","tipo":"nota","texto":"# Tela"},{"ref":"b","tipo":"codigo","texto":"tela -> api"},{"tipo":"ligacao","de":"a","para":"b","texto":"chama"}]}`
	if texto, erro := resultado(t, c.chamar(21, "acrescentar_a_lousa", itens)); erro || !strings.Contains(texto, "Acrescentei 3 itens") {
		t.Errorf("acrescentar_a_lousa: %q %v", texto, erro)
	}
	corpo, _ := json.Marshal(nucleo.corpos[len(nucleo.corpos)-1])
	if !strings.Contains(string(corpo), `"de":"a"`) || !strings.Contains(string(corpo), `"elementos":[`) {
		t.Errorf("corpo levado ao núcleo: %s", corpo)
	}
	antes = len(nucleo.pedidos)
	for _, ruim := range []string{`{"itens":[]}`, `{"itens":[{"tipo":"nota","mover":true}]}`, `{"itens":[{"tipo":"nota"}],"tarefa":2}`} {
		if texto, erro := resultado(t, c.chamar(22, "acrescentar_a_lousa", ruim)); !erro {
			t.Errorf("argumentos ruins aceitos (%s): %q", ruim, texto)
		}
	}
	if len(nucleo.pedidos) != antes {
		t.Errorf("argumentos ruins da lousa chegaram ao núcleo")
	}
	for _, f := range ferramentas {
		if f := f.(map[string]any); f["name"] == "acrescentar_a_lousa" {
			props := f["inputSchema"].(map[string]any)["properties"].(map[string]any)
			item := props["itens"].(map[string]any)["items"].(map[string]any)
			if item["additionalProperties"] != false || item["properties"].(map[string]any)["tipo"] == nil {
				t.Errorf("esquema do item da lousa: %v", item)
			}
		}
	}

	// Ferramenta inexistente e método desconhecido: erros do protocolo.
	if e := c.chamar(10, "apagar_tudo", `{}`)["error"].(map[string]any); e["code"] != float64(erroParametros) {
		t.Errorf("ferramenta inexistente: %v", e)
	}
	c.mandar(`{"jsonrpc":"2.0","id":11,"method":"resources/list"}`)
	if e := c.receber()["error"].(map[string]any); e["code"] != float64(erroMetodo) {
		t.Errorf("método desconhecido: %v", e)
	}
	c.mandar(`{"jsonrpc":"2.0","id":12,"method":"ping"}`)
	if r := c.receber(); r["result"] == nil {
		t.Errorf("ping: %v", r)
	}
	c.mandar(`isto não é json`)
	if e := c.receber()["error"].(map[string]any); e["code"] != float64(erroLeitura) {
		t.Errorf("json inválido: %v", e)
	}
	// Uma linha grande demais é descartada inteira e o servidor continua.
	c.mandar(`{"jsonrpc":"2.0","id":13,"method":"ping","params":{"x":"` + strings.Repeat("a", MaxLinha) + `"}}`)
	if e := c.receber()["error"].(map[string]any); e["code"] != float64(erroPedido) {
		t.Errorf("linha grande: %v", e)
	}
	c.mandar(`{"jsonrpc":"2.0","id":14,"method":"ping"}`)
	if r := c.receber(); r["id"] != 14.0 {
		t.Errorf("depois da linha grande: %v", r)
	}
	c.envia.Close()
	if err := <-c.fim; err != nil {
		t.Errorf("fim: %v", err)
	}
}
