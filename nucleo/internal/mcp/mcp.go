// Package mcp é o servidor MCP que a Colmeia passa aos agentes do Claude Code
// que ela abre (`colmeia-nucleo mcp`): JSON-RPC 2.0, uma mensagem por linha,
// no stdin e no stdout. Cada ferramenta vira um pedido às rotas /v1/agente/*
// do núcleo, pelo socket local e com o token do próprio agente: a tarefa sai
// do token, e o núcleo confere tudo. Aqui não há regra de negócio nem acesso
// a arquivos; o processo fica parado na leitura do stdin até chegar pedido.
package mcp

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/url"
	"strconv"
	"strings"
	"sync"
)

// MaxLinha aceita do cliente.
const MaxLinha = 1 << 20

// Versões do protocolo que este servidor fala, da mais nova para a mais antiga.
var Versoes = []string{"2025-06-18", "2025-03-26", "2024-11-05"}

// Cliente faz os pedidos ao núcleo. corpo nil vai sem corpo.
type Cliente interface {
	Pedir(ctx context.Context, metodo, caminho string, corpo any) (status int, resposta []byte, err error)
}

// Servidor atende um cliente MCP.
type Servidor struct {
	Nucleo Cliente
	Versao string

	muSaida sync.Mutex
	saida   io.Writer
}

type mensagem struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Metodo  string          `json:"method,omitempty"`
	Params  json.RawMessage `json:"params,omitempty"`
}

type erroRPC struct {
	Codigo   int    `json:"code"`
	Mensagem string `json:"message"`
}

const (
	erroLeitura      = -32700
	erroPedido       = -32600
	erroMetodo       = -32601
	erroParametros   = -32602
	erroInterno      = -32603
	instrucoesGerais = "Ferramentas da Colmeia para a tarefa em que você trabalha: ler a tarefa e a nota da daily ou da sprint, " +
		"complementar a nota com o que foi pedido, anexar imagens e abrir e capturar o navegador da tarefa. " +
		"Quando um pedido da daily chegar no terminal, responda pela nota (complementar_nota com o número do pedido), " +
		"anexe as capturas e só então chame concluir_pedido: é ele que avisa o usuário. Seja breve: a nota aparece num slide."
)

// Servir lê os pedidos até o fim da entrada.
func (s *Servidor) Servir(ctx context.Context, entrada io.Reader, saida io.Writer) error {
	s.saida = saida
	leitor := bufio.NewReaderSize(entrada, 64<<10)
	for {
		linha, grande, err := lerLinha(leitor)
		if grande {
			s.responderErro(nil, erroPedido, "mensagem grande demais (até 1 MB)")
		} else if len(bytes.TrimSpace(linha)) > 0 {
			s.tratar(ctx, linha)
		}
		if err != nil {
			if errors.Is(err, io.EOF) {
				return nil
			}
			return err
		}
	}
}

// lerLinha lê até o fim da linha; uma linha acima de MaxLinha é descartada
// inteira (grande = true), sem guardar o excesso na memória.
func lerLinha(r *bufio.Reader) ([]byte, bool, error) {
	var linha []byte
	grande := false
	for {
		pedaco, err := r.ReadSlice('\n')
		if !grande {
			if len(linha)+len(pedaco) > MaxLinha {
				grande, linha = true, nil
			} else {
				linha = append(linha, pedaco...)
			}
		}
		if errors.Is(err, bufio.ErrBufferFull) {
			continue
		}
		return linha, grande, err
	}
}

func (s *Servidor) escrever(v any) {
	bruto, err := json.Marshal(v)
	if err != nil {
		return
	}
	s.muSaida.Lock()
	defer s.muSaida.Unlock()
	s.saida.Write(append(bruto, '\n'))
}

func (s *Servidor) responder(id json.RawMessage, resultado any) {
	s.escrever(map[string]any{"jsonrpc": "2.0", "id": id, "result": resultado})
}

func (s *Servidor) responderErro(id json.RawMessage, codigo int, texto string) {
	if id == nil {
		id = json.RawMessage("null")
	}
	s.escrever(map[string]any{"jsonrpc": "2.0", "id": id, "error": erroRPC{codigo, texto}})
}

func (s *Servidor) tratar(ctx context.Context, linha []byte) {
	var m mensagem
	if err := json.Unmarshal(linha, &m); err != nil {
		if bytes.HasPrefix(bytes.TrimSpace(linha), []byte("[")) {
			s.responderErro(nil, erroPedido, "lotes não são aceitos")
			return
		}
		s.responderErro(nil, erroLeitura, "JSON inválido")
		return
	}
	if m.JSONRPC != "2.0" {
		s.responderErro(m.ID, erroPedido, "só JSON-RPC 2.0")
		return
	}
	notificacao := len(m.ID) == 0 || string(m.ID) == "null"
	if m.Metodo == "" {
		return // resposta a um pedido nosso (não fazemos nenhum)
	}
	if notificacao {
		return // notifications/initialized, notifications/cancelled: nada a fazer
	}
	switch m.Metodo {
	case "initialize":
		var p struct {
			Versao string `json:"protocolVersion"`
		}
		json.Unmarshal(m.Params, &p)
		versao := Versoes[0]
		for _, v := range Versoes {
			if v == p.Versao {
				versao = v
			}
		}
		s.responder(m.ID, map[string]any{
			"protocolVersion": versao,
			"capabilities":    map[string]any{"tools": map[string]any{"listChanged": false}},
			"serverInfo":      map[string]any{"name": "colmeia", "title": "Colmeia", "version": s.Versao},
			"instructions":    instrucoesGerais,
		})
	case "ping":
		s.responder(m.ID, map[string]any{})
	case "tools/list":
		s.responder(m.ID, map[string]any{"tools": Ferramentas})
	case "tools/call":
		var p struct {
			Nome       string          `json:"name"`
			Argumentos json.RawMessage `json:"arguments"`
		}
		if err := json.Unmarshal(m.Params, &p); err != nil {
			s.responderErro(m.ID, erroParametros, "parâmetros inválidos")
			return
		}
		f, ok := porNome[p.Nome]
		if !ok {
			s.responderErro(m.ID, erroParametros, "ferramenta desconhecida: "+p.Nome)
			return
		}
		conteudo, err := f.chamar(ctx, s.Nucleo, p.Argumentos)
		if err != nil {
			s.responder(m.ID, map[string]any{"content": []any{texto(err.Error())}, "isError": true})
			return
		}
		s.responder(m.ID, map[string]any{"content": conteudo})
	default:
		s.responderErro(m.ID, erroMetodo, "método desconhecido: "+m.Metodo)
	}
}

func texto(t string) map[string]any { return map[string]any{"type": "text", "text": t} }

// Ferramenta é como cada ferramenta aparece em tools/list.
type Ferramenta struct {
	Nome      string         `json:"name"`
	Titulo    string         `json:"title"`
	Descricao string         `json:"description"`
	Entrada   map[string]any `json:"inputSchema"`
	chamar    func(ctx context.Context, n Cliente, args json.RawMessage) ([]any, error)
}

// argumentos lê os argumentos estritamente: campo desconhecido é erro.
func argumentos(bruto json.RawMessage, destino any) error {
	if len(bytes.TrimSpace(bruto)) == 0 || string(bytes.TrimSpace(bruto)) == "null" {
		bruto = json.RawMessage("{}")
	}
	d := json.NewDecoder(bytes.NewReader(bruto))
	d.DisallowUnknownFields()
	if err := d.Decode(destino); err != nil {
		return fmt.Errorf("argumentos inválidos: %v", err)
	}
	if d.More() {
		return errors.New("argumentos inválidos: sobra depois do objeto")
	}
	return nil
}

// pedirJSON faz o pedido e devolve o JSON da resposta, ou o erro do núcleo.
func pedirJSON(ctx context.Context, n Cliente, metodo, caminho string, corpo any) (map[string]any, error) {
	status, bruto, err := n.Pedir(ctx, metodo, caminho, corpo)
	if err != nil {
		return nil, fmt.Errorf("sem resposta do núcleo da Colmeia: %v", err)
	}
	var r map[string]any
	json.Unmarshal(bruto, &r)
	if status != 200 {
		if msg, ok := r["erro"].(string); ok && msg != "" {
			return nil, errors.New(msg)
		}
		if status == 401 {
			return nil, errors.New("a Colmeia não reconhece mais este agente (ele foi reiniciado ou removido)")
		}
		return nil, fmt.Errorf("o núcleo respondeu %d", status)
	}
	return r, nil
}

func comoTexto(r map[string]any) []any {
	bruto, _ := json.MarshalIndent(r, "", "  ")
	return []any{texto(string(bruto))}
}

var propTipo = map[string]any{"type": "string", "enum": []string{"daily", "sprint"},
	"description": "daily ou sprint. Sem ele, vale a nota do pedido aberto mais recente ou a daily de hoje."}
var propPeriodo = map[string]any{"type": "string",
	"description": "AAAA-MM-DD (daily) ou AAAA-MM-DD..AAAA-MM-DD (sprint). Sem ele, o do pedido ou hoje."}

func objeto(props map[string]any, obrigatorios ...string) map[string]any {
	o := map[string]any{"type": "object", "properties": props, "additionalProperties": false}
	if len(obrigatorios) > 0 {
		o["required"] = obrigatorios
	}
	return o
}

// Ferramentas oferecidas aos agentes.
var Ferramentas = []Ferramenta{
	{
		Nome: "ler_tarefa", Titulo: "Ler a tarefa",
		Descricao: "Mostra a tarefa em que você trabalha na Colmeia: título, projeto, branch, pasta, coluna, a nota da daily de hoje, a última nota de sprint e os pedidos abertos.",
		Entrada:   objeto(map[string]any{}),
		chamar: func(ctx context.Context, n Cliente, args json.RawMessage) ([]any, error) {
			var a struct{}
			if err := argumentos(args, &a); err != nil {
				return nil, err
			}
			r, err := pedirJSON(ctx, n, "GET", "/v1/agente/tarefa", nil)
			if err != nil {
				return nil, err
			}
			return comoTexto(r), nil
		},
	},
	{
		Nome: "ler_nota", Titulo: "Ler a nota",
		Descricao: "Lê a nota da tarefa na daily ou na sprint.",
		Entrada:   objeto(map[string]any{"tipo": propTipo, "periodo": propPeriodo}),
		chamar: func(ctx context.Context, n Cliente, args json.RawMessage) ([]any, error) {
			var a struct {
				Tipo    string `json:"tipo"`
				Periodo string `json:"periodo"`
			}
			if err := argumentos(args, &a); err != nil {
				return nil, err
			}
			q := url.Values{}
			if a.Tipo != "" {
				q.Set("tipo", a.Tipo)
			}
			if a.Periodo != "" {
				q.Set("periodo", a.Periodo)
			}
			caminho := "/v1/agente/nota"
			if len(q) > 0 {
				caminho += "?" + q.Encode()
			}
			r, err := pedirJSON(ctx, n, "GET", caminho, nil)
			if err != nil {
				return nil, err
			}
			return comoTexto(r), nil
		},
	},
	{
		Nome: "complementar_nota", Titulo: "Complementar a nota",
		Descricao: "Acrescenta texto ao fim da nota da tarefa (daily ou sprint), depois de uma linha em branco. Use para responder um pedido da daily: " +
			"informe o número do pedido. Seja breve (até 5 linhas): a nota aparece num slide e tem limite de 4.000 caracteres.",
		Entrada: objeto(map[string]any{
			"texto":   map[string]any{"type": "string", "description": "O que acrescentar."},
			"tipo":    propTipo,
			"periodo": propPeriodo,
			"pedido":  map[string]any{"type": "integer", "description": "Número do pedido que este texto responde (o pedido só fecha com concluir_pedido)."},
		}, "texto"),
		chamar: func(ctx context.Context, n Cliente, args json.RawMessage) ([]any, error) {
			return gravarNota(ctx, n, args, "complementar")
		},
	},
	{
		Nome: "escrever_nota", Titulo: "Reescrever a nota",
		Descricao: "Substitui a nota inteira da tarefa. Prefira complementar_nota: reescrever apaga o que o usuário escreveu.",
		Entrada: objeto(map[string]any{
			"texto": map[string]any{"type": "string", "description": "A nota inteira, nova."}, "tipo": propTipo, "periodo": propPeriodo,
		}, "texto"),
		chamar: func(ctx context.Context, n Cliente, args json.RawMessage) ([]any, error) {
			return gravarNota(ctx, n, args, "substituir")
		},
	},
	{
		Nome: "anexar_imagem", Titulo: "Anexar imagem à tarefa",
		Descricao: "Anexa à tarefa (e aos slides da daily) uma imagem PNG ou JPEG que está dentro da pasta da tarefa.",
		Entrada: objeto(map[string]any{
			"caminho": map[string]any{"type": "string", "description": "Caminho do arquivo, relativo à pasta da tarefa (ou absoluto, de dentro dela)."},
			"legenda": map[string]any{"type": "string", "description": "Legenda curta (opcional)."},
		}, "caminho"),
		chamar: func(ctx context.Context, n Cliente, args json.RawMessage) ([]any, error) {
			var a struct {
				Caminho string `json:"caminho"`
				Legenda string `json:"legenda"`
			}
			if err := argumentos(args, &a); err != nil {
				return nil, err
			}
			r, err := pedirJSON(ctx, n, "POST", "/v1/agente/anexos", a)
			if err != nil {
				return nil, err
			}
			return []any{texto(fmt.Sprintf("Imagem anexada à tarefa (anexo %v).", r["id"]))}, nil
		},
	},
	{
		Nome: "abrir_navegador", Titulo: "Abrir no navegador da tarefa",
		Descricao: "Abre um endereço http ou https, ou um arquivo da pasta da tarefa (file://), no navegador que a Colmeia controla para esta tarefa, " +
			"ao lado da janela dela. Espera a página carregar.",
		Entrada: objeto(map[string]any{"url": map[string]any{"type": "string", "description": "Ex.: http://localhost:5173/pedidos"}}, "url"),
		chamar: func(ctx context.Context, n Cliente, args json.RawMessage) ([]any, error) {
			var a struct {
				URL string `json:"url"`
			}
			if err := argumentos(args, &a); err != nil {
				return nil, err
			}
			r, err := pedirJSON(ctx, n, "POST", "/v1/agente/navegador", a)
			if err != nil {
				return nil, err
			}
			return []any{texto(fmt.Sprintf("Aberto no navegador: %v", r["descricao"]))}, nil
		},
	},
	{
		Nome: "capturar_navegador", Titulo: "Capturar o navegador da tarefa",
		Descricao: "Tira um print do que o navegador da tarefa mostra agora. Por padrão anexa à tarefa (aparece no slide da daily) e devolve a imagem para você ver.",
		Entrada: objeto(map[string]any{
			"anexar":  map[string]any{"type": "boolean", "description": "Anexar à tarefa (padrão: sim)."},
			"legenda": map[string]any{"type": "string", "description": "Legenda curta (opcional)."},
		}),
		chamar: func(ctx context.Context, n Cliente, args json.RawMessage) ([]any, error) {
			a := struct {
				Anexar  *bool  `json:"anexar"`
				Legenda string `json:"legenda"`
			}{}
			if err := argumentos(args, &a); err != nil {
				return nil, err
			}
			anexar := a.Anexar == nil || *a.Anexar
			r, err := pedirJSON(ctx, n, "POST", "/v1/agente/navegador/captura", map[string]any{"anexar": anexar, "legenda": a.Legenda})
			if err != nil {
				return nil, err
			}
			png, _ := r["png"].(string)
			descricao := "Captura do navegador"
			if id, ok := r["anexo"].(float64); ok && id > 0 {
				descricao += " anexada à tarefa (anexo " + strconv.FormatInt(int64(id), 10) + ")"
			}
			return []any{map[string]any{"type": "image", "data": png, "mimeType": "image/png"}, texto(descricao + ".")}, nil
		},
	},
	{
		Nome: "concluir_pedido", Titulo: "Concluir um pedido",
		Descricao: "Marca como respondido um pedido feito pela daily ou pela sprint e avisa o usuário. Chame por último, depois da nota e das capturas. " +
			"Com resumo, acrescenta o resumo à nota do pedido.",
		Entrada: objeto(map[string]any{
			"pedido": map[string]any{"type": "integer", "description": "Número do pedido."},
			"resumo": map[string]any{"type": "string", "description": "Resumo curto para acrescentar à nota (opcional)."},
		}, "pedido"),
		chamar: func(ctx context.Context, n Cliente, args json.RawMessage) ([]any, error) {
			var a struct {
				Pedido int64  `json:"pedido"`
				Resumo string `json:"resumo"`
			}
			if err := argumentos(args, &a); err != nil {
				return nil, err
			}
			if a.Pedido <= 0 {
				return nil, errors.New("informe o número do pedido")
			}
			if _, err := pedirJSON(ctx, n, "POST", "/v1/agente/pedidos/"+strconv.FormatInt(a.Pedido, 10)+"/concluir", map[string]any{"resumo": a.Resumo}); err != nil {
				return nil, err
			}
			return []any{texto(fmt.Sprintf("Pedido %d respondido.", a.Pedido))}, nil
		},
	},
}

func gravarNota(ctx context.Context, n Cliente, args json.RawMessage, modo string) ([]any, error) {
	var a struct {
		Texto   string `json:"texto"`
		Tipo    string `json:"tipo"`
		Periodo string `json:"periodo"`
		Pedido  int64  `json:"pedido"`
	}
	if modo == "substituir" {
		var s struct {
			Texto   string `json:"texto"`
			Tipo    string `json:"tipo"`
			Periodo string `json:"periodo"`
		}
		if err := argumentos(args, &s); err != nil {
			return nil, err
		}
		a.Texto, a.Tipo, a.Periodo = s.Texto, s.Tipo, s.Periodo
	} else if err := argumentos(args, &a); err != nil {
		return nil, err
	}
	if strings.TrimSpace(a.Texto) == "" && modo == "complementar" {
		return nil, errors.New("o texto está vazio")
	}
	corpo := map[string]any{"texto": a.Texto, "tipo": a.Tipo, "periodo": a.Periodo, "modo": modo}
	if a.Pedido > 0 {
		corpo["pedido"] = a.Pedido
	}
	r, err := pedirJSON(ctx, n, "PUT", "/v1/agente/nota", corpo)
	if err != nil {
		return nil, err
	}
	return []any{texto(fmt.Sprintf("Nota da %v (%v) gravada.", r["tipo"], r["periodo"]))}, nil
}

var porNome = func() map[string]Ferramenta {
	m := map[string]Ferramenta{}
	for _, f := range Ferramentas {
		m[f.Nome] = f
	}
	return m
}()
