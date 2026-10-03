package api

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"image/png"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/anexos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/arquivos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/canal"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/navegador"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

// LadoParaOAgente: a captura devolvida ao agente é reduzida até caber nisso
// (o que o modelo enxerga sem cortar).
const LadoParaOAgente = 1568

// Rotas dos agentes. Só aceitam o token de um agente (canal.Autenticar), e
// nenhuma leva id na URL: a tarefa sai do token, que aponta para o agente e
// dele para a tarefa. Não há como falar de outra tarefa por aqui.
func (s *Servidor) rotasAgente(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/agente/tarefa", s.doAgente(s.agenteTarefa))
	mux.HandleFunc("GET /v1/agente/nota", s.doAgente(s.agenteLerNota))
	mux.HandleFunc("PUT /v1/agente/nota", s.doAgente(s.agenteGravarNota))
	mux.HandleFunc("POST /v1/agente/anexos", s.doAgente(s.agenteAnexar))
	mux.HandleFunc("POST /v1/agente/navegador", s.doAgente(s.agenteAbrirNavegador))
	mux.HandleFunc("POST /v1/agente/navegador/captura", s.doAgente(s.agenteCapturar))
	mux.HandleFunc("POST /v1/agente/pedidos/{id}/concluir", s.doAgente(s.agenteConcluirPedido))
}

// quemAgente é o agente que pediu, com a tarefa dele.
type quemAgente struct {
	ctx     dados.ContextoAgente
	tarefa  dados.Tarefa
	projeto dados.Projeto
	perfil  int64
	pasta   string
}

// doAgente só deixa passar pedidos feitos com o token de um agente que ainda
// existe, e entrega ao tratador a tarefa dele.
func (s *Servidor) doAgente(f func(http.ResponseWriter, *http.Request, quemAgente)) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		quem, ok := canal.QuemPediu(r.Context())
		if !ok || quem.Agente == 0 {
			http.Error(w, "não autorizado", http.StatusUnauthorized)
			return
		}
		c, err := s.Banco.ContextoDoAgente(r.Context(), quem.Agente)
		if err != nil {
			http.Error(w, "não autorizado", http.StatusUnauthorized)
			return
		}
		t, p, perfil, err := s.Banco.Tarefa(r.Context(), c.TarefaID)
		if err != nil {
			responderErro(w, err)
			return
		}
		f(w, r, quemAgente{ctx: c, tarefa: t, projeto: p, perfil: perfil, pasta: t.Pasta(p)})
	}
}

func hoje() string { return time.Now().Format("2006-01-02") }

// notaPadrao: sem tipo e período, vale a nota do pedido aberto mais recente
// da tarefa; sem pedido, a daily de hoje.
func (s *Servidor) notaPadrao(ctx context.Context, tarefa int64, tipo, periodo string) (string, string, error) {
	if tipo != "" && periodo != "" {
		return tipo, periodo, nil
	}
	abertos, err := s.Banco.PedidosAbertos(ctx, tarefa)
	if err != nil {
		return "", "", err
	}
	if n := len(abertos); n > 0 && (tipo == "" || tipo == abertos[n-1].Tipo) {
		return abertos[n-1].Tipo, abertos[n-1].Periodo, nil
	}
	if tipo == "" || tipo == "daily" {
		return "daily", hoje(), nil
	}
	// Sprint sem período: a última nota de sprint da tarefa.
	ultimas, err := s.Banco.UltimasNotas(ctx, []int64{tarefa}, "sprint")
	if err != nil {
		return "", "", err
	}
	if n, ok := ultimas[tarefa]; ok {
		return "sprint", n.Periodo, nil
	}
	return "", "", dados.ErrInvalido{Motivo: "informe o período da sprint (AAAA-MM-DD..AAAA-MM-DD)"}
}

func (s *Servidor) agenteTarefa(w http.ResponseWriter, r *http.Request, q quemAgente) {
	ctx := r.Context()
	daily, err := s.Banco.Nota(ctx, q.tarefa.ID, "daily", hoje())
	if err != nil {
		responderErro(w, err)
		return
	}
	resposta := map[string]any{
		"tarefa": map[string]any{"id": q.tarefa.ID, "titulo": q.tarefa.Titulo, "projeto": q.projeto.Nome, "branch": q.tarefa.Branch,
			"pasta": q.pasta, "coluna": q.tarefa.Coluna},
		"hoje":       hoje(),
		"nota_daily": daily,
		"navegador":  s.Navegadores.Aberto(q.perfil, q.tarefa.ID),
	}
	if ultimas, err := s.Banco.UltimasNotas(ctx, []int64{q.tarefa.ID}, "sprint"); err == nil {
		if n, ok := ultimas[q.tarefa.ID]; ok {
			resposta["nota_sprint"] = n
		}
	}
	abertos, err := s.Banco.PedidosAbertos(ctx, q.tarefa.ID)
	if err != nil {
		responderErro(w, err)
		return
	}
	resposta["pedidos"] = abertos
	responderJSON(w, resposta)
}

func (s *Servidor) agenteLerNota(w http.ResponseWriter, r *http.Request, q quemAgente) {
	tipo, periodo, err := s.notaPadrao(r.Context(), q.tarefa.ID, r.URL.Query().Get("tipo"), r.URL.Query().Get("periodo"))
	if err != nil {
		responderErro(w, err)
		return
	}
	nota, err := s.Banco.Nota(r.Context(), q.tarefa.ID, tipo, periodo)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, nota)
}

// pedidoDaTarefa lê o pedido e confere que ele é da tarefa do agente.
func (s *Servidor) pedidoDaTarefa(ctx context.Context, id, tarefa int64) (dados.Pedido, error) {
	p, err := s.Banco.Pedido(ctx, id)
	if err != nil || p.TarefaID != tarefa {
		return dados.Pedido{}, dados.ErrInvalido{Motivo: "o pedido " + strconv.FormatInt(id, 10) + " não é desta tarefa"}
	}
	return p, nil
}

func (s *Servidor) agenteGravarNota(w http.ResponseWriter, r *http.Request, q quemAgente) {
	var pedido struct {
		Texto   string `json:"texto"`
		Tipo    string `json:"tipo"`
		Periodo string `json:"periodo"`
		Modo    string `json:"modo"`
		Pedido  int64  `json:"pedido"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	ctx := r.Context()
	var doPedido dados.Pedido
	if pedido.Pedido != 0 {
		p, err := s.pedidoDaTarefa(ctx, pedido.Pedido, q.tarefa.ID)
		if err != nil {
			responderErro(w, err)
			return
		}
		doPedido = p
		if pedido.Tipo == "" {
			pedido.Tipo = p.Tipo
		}
		if pedido.Periodo == "" && pedido.Tipo == p.Tipo {
			pedido.Periodo = p.Periodo
		}
	}
	tipo, periodo, err := s.notaPadrao(ctx, q.tarefa.ID, pedido.Tipo, pedido.Periodo)
	if err != nil {
		responderErro(w, err)
		return
	}
	nota, err := s.Banco.GravarNota(ctx, dados.GravacaoNota{Tarefa: q.tarefa.ID, Tipo: tipo, Periodo: periodo, Texto: pedido.Texto,
		Modo: pedido.Modo, Agente: q.ctx.ID})
	if err != nil {
		responderErro(w, err)
		return
	}
	// A nota com o número do pedido não fecha o pedido: o agente ainda pode
	// anexar a captura, e a tela só deve dizer "respondeu" com tudo na nota.
	// Fecha com concluir_pedido ou, se ele esquecer, quando terminar a vez.
	if doPedido.ID != 0 && doPedido.Aberto() {
		s.marcarRespondendo(doPedido.ID)
	}
	responderJSON(w, nota)
}

// legendaValida confere a legenda (curta, sem controle e sem segredo).
func legendaValida(legenda string) error {
	if legenda == "" {
		return nil
	}
	if segredos.Parece(legenda) {
		return dados.ErrInvalido{Motivo: "a legenda parece ter uma senha ou chave"}
	}
	return nil
}

func (s *Servidor) agenteAnexar(w http.ResponseWriter, r *http.Request, q quemAgente) {
	var pedido struct {
		Caminho string `json:"caminho"`
		Legenda string `json:"legenda"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if err := legendaValida(pedido.Legenda); err != nil {
		responderErro(w, err)
		return
	}
	bruto, tipo, err := arquivos.LerParaAnexo(q.pasta, pedido.Caminho)
	if err != nil {
		responderErro(w, erroDeArquivo(err))
		return
	}
	anexo, err := s.gravarAnexo(r.Context(), bruto, tipo, dados.NovoAnexo{Perfil: q.perfil, Tarefa: q.tarefa.ID, Agente: q.ctx.ID, Origem: "arquivo",
		Legenda: pedido.Legenda, Nome: filepath.Base(pedido.Caminho)})
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"id": anexo.ID, "largura": anexo.Largura, "altura": anexo.Altura})
}

// gravarAnexo grava a imagem (PNG ou JPEG) e cria o anexo.
func (s *Servidor) gravarAnexo(ctx context.Context, bruto []byte, tipo string, novo dados.NovoAnexo) (dados.Anexo, error) {
	imagem, err := anexos.GravarImagem(s.pastaAnexos(novo.Perfil), bytes.NewReader(bruto), tipo)
	if errors.Is(err, anexos.ErrGrande) || errors.Is(err, anexos.ErrDimensoes) || errors.Is(err, anexos.ErrNaoPNG) || errors.Is(err, anexos.ErrNaoJPEG) {
		return dados.Anexo{}, dados.ErrInvalido{Motivo: err.Error()}
	}
	if err != nil {
		return dados.Anexo{}, err
	}
	novo.Sha256, novo.Largura, novo.Altura, novo.Bytes = imagem.Sha256, imagem.Largura, imagem.Altura, imagem.Bytes
	return s.Banco.CriarAnexo(ctx, novo)
}

// erroDeArquivo transforma os erros de arquivos em mensagens para quem pediu.
func erroDeArquivo(err error) error {
	if arquivos.Erro(err) {
		return dados.ErrInvalido{Motivo: err.Error()}
	}
	return err
}

func (s *Servidor) agenteAbrirNavegador(w http.ResponseWriter, r *http.Request, q quemAgente) {
	var pedido struct {
		URL string `json:"url"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	// O agente abre sem tomar o foco: o usuário pode estar apresentando.
	u, err := s.abrirNavegador(r.Context(), q.perfil, q.tarefa.ID, q.pasta, pedido.URL, nil, true, q.ctx.ID)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"descricao": u.Descricao})
}

func (s *Servidor) agenteCapturar(w http.ResponseWriter, r *http.Request, q quemAgente) {
	pedido := struct {
		Anexar  *bool  `json:"anexar"`
		Legenda string `json:"legenda"`
	}{}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	anexar := pedido.Anexar == nil || *pedido.Anexar
	captura, anexo, err := s.capturarNavegador(r.Context(), q.perfil, q.tarefa.ID, q.ctx.ID, anexar, pedido.Legenda)
	if err != nil {
		responderErro(w, err)
		return
	}
	// O agente recebe a imagem reduzida, para enxergar sem cortes.
	img, err := png.Decode(bytes.NewReader(captura))
	if err != nil {
		responderErro(w, dados.ErrInvalido{Motivo: "o navegador devolveu uma captura inválida"})
		return
	}
	var reduzida bytes.Buffer
	png.Encode(&reduzida, anexos.Reduzir(img, LadoParaOAgente))
	responderJSON(w, map[string]any{"png": base64.StdEncoding.EncodeToString(reduzida.Bytes()), "anexo": anexo})
}

func (s *Servidor) agenteConcluirPedido(w http.ResponseWriter, r *http.Request, q quemAgente) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Resumo string `json:"resumo"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	ctx := r.Context()
	p, err := s.pedidoDaTarefa(ctx, id, q.tarefa.ID)
	if err != nil {
		responderErro(w, err)
		return
	}
	if p.Estado == dados.PedidoRespondido {
		responderJSON(w, p)
		return
	}
	if !p.Aberto() {
		responderErro(w, dados.ErrPedidoFechado)
		return
	}
	if pedido.Resumo != "" {
		if _, err := s.Banco.GravarNota(ctx, dados.GravacaoNota{Tarefa: q.tarefa.ID, Tipo: p.Tipo, Periodo: p.Periodo, Texto: pedido.Resumo,
			Modo: dados.ModoComplementar, Agente: q.ctx.ID}); err != nil {
			responderErro(w, err)
			return
		}
	}
	p, err = s.responderPedido(ctx, id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, p)
}

// MCP dos agentes do Claude Code

// pastaAgentes guarda o token e a configuração MCP de cada agente: dentro do
// diretório do canal (runtime, tmpfs), 0700.
func (s *Servidor) pastaAgentes() (string, error) {
	dir := filepath.Join(s.DirCanal, "agentes")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	return dir, os.Chmod(dir, 0o700)
}

// gravarPrivado escreve o arquivo com 0600, trocando de uma vez.
func gravarPrivado(dir, nome string, conteudo []byte) (string, error) {
	temporario, err := os.CreateTemp(dir, "."+nome+"-*")
	if err != nil {
		return "", err
	}
	defer os.Remove(temporario.Name())
	if err := temporario.Chmod(0o600); err != nil {
		temporario.Close()
		return "", err
	}
	if _, err := temporario.Write(conteudo); err != nil {
		temporario.Close()
		return "", err
	}
	if err := temporario.Close(); err != nil {
		return "", err
	}
	final := filepath.Join(dir, nome)
	return final, os.Rename(temporario.Name(), final)
}

// prepararMCP gera o token do agente e grava a configuração MCP que o Claude
// Code recebe por --mcp-config. Devolve os argumentos a acrescentar. O token
// fica só num arquivo 0600: não vai em argumento nem em variável de ambiente.
func (s *Servidor) prepararMCP(agente int64) ([]string, error) {
	if s.DirCanal == "" || s.Executavel == "" {
		return nil, nil
	}
	dir, err := s.pastaAgentes()
	if err != nil {
		return nil, err
	}
	token, err := s.Fichas.Emitir(agente)
	if err != nil {
		return nil, err
	}
	id := strconv.FormatInt(agente, 10)
	arquivoToken, err := gravarPrivado(dir, id+".token", []byte(token))
	if err != nil {
		s.Fichas.Revogar(agente)
		return nil, err
	}
	config := map[string]any{"mcpServers": map[string]any{"colmeia": map[string]any{
		"type": "stdio", "command": s.Executavel,
		"args": []string{"mcp", "--socket", filepath.Join(s.DirCanal, canal.NomeSocket), "--token-arquivo", arquivoToken},
	}}}
	bruto, _ := json.MarshalIndent(config, "", "  ")
	arquivoConfig, err := gravarPrivado(dir, id+".mcp.json", bruto)
	if err != nil {
		s.limparMCP(agente)
		return nil, err
	}
	return []string{"--mcp-config", arquivoConfig, "--allowedTools", "mcp__colmeia"}, nil
}

// limparMCP revoga o token do agente e apaga os arquivos dele.
func (s *Servidor) limparMCP(agente int64) {
	s.Fichas.Revogar(agente)
	if s.DirCanal == "" {
		return
	}
	id := strconv.FormatInt(agente, 10)
	for _, nome := range []string{id + ".token", id + ".mcp.json"} {
		os.Remove(filepath.Join(s.DirCanal, "agentes", nome))
	}
}

// LimparAgentesAntigos apaga os tokens e configurações que sobraram de um
// núcleo anterior (os tokens já não valem: ficam só em memória).
func LimparAgentesAntigos(dirCanal string) {
	entradas, err := os.ReadDir(filepath.Join(dirCanal, "agentes"))
	if err != nil {
		return
	}
	for _, e := range entradas {
		os.Remove(filepath.Join(dirCanal, "agentes", e.Name()))
	}
}

// abrirNavegador confere o endereço, abre a janela da tarefa e grava o evento.
func (s *Servidor) abrirNavegador(ctx context.Context, perfil, tarefa int64, pasta, bruto string, geo *navegador.Geometria, fundo bool, agente int64) (navegador.URL, error) {
	var u navegador.URL
	if bruto != "" {
		var err error
		if u, err = navegador.ValidarURL(bruto, pasta); err != nil {
			return u, dados.ErrInvalido{Motivo: err.Error()}
		}
	}
	nova, err := s.Navegadores.Abrir(ctx, navegador.Pedido{Perfil: perfil, Tarefa: tarefa, Pasta: pasta, URL: u, Geometria: geo, Fundo: fundo})
	var endereco navegador.ErrEndereco
	if errors.Is(err, navegador.ErrSemNavegador) || errors.As(err, &endereco) {
		return u, dados.ErrInvalido{Motivo: err.Error()}
	}
	if err != nil {
		return u, dados.ErrInvalido{Motivo: "o navegador não abriu: " + err.Error()}
	}
	if nova || u.Descricao != "" {
		if u.Descricao == "" {
			u.Descricao = s.Navegadores.Aberto(perfil, tarefa).Descricao
		}
		s.registrarNavegador(context.WithoutCancel(ctx), "navegador.aberto", tarefa, agente, map[string]any{"descricao": u.Descricao})
	}
	return u, nil
}

// capturarNavegador tira o PNG da janela da tarefa e, se pedido, anexa à
// tarefa (origem captura, legenda "Navegador: <endereço>").
func (s *Servidor) capturarNavegador(ctx context.Context, perfil, tarefa, agente int64, anexar bool, legenda string) ([]byte, int64, error) {
	if err := legendaValida(legenda); err != nil {
		return nil, 0, err
	}
	captura, descricao, err := s.Navegadores.Capturar(ctx, perfil, tarefa)
	if errors.Is(err, navegador.ErrFechado) {
		return nil, 0, dados.ErrInvalido{Motivo: "O navegador desta tarefa não está aberto. Abra um endereço antes de capturar."}
	}
	if errors.Is(err, navegador.ErrSaiuDaPasta) {
		s.registrarNavegador(context.WithoutCancel(ctx), "navegador.recusado", tarefa, agente, map[string]any{})
		return nil, 0, dados.ErrInvalido{Motivo: err.Error()}
	}
	if err != nil {
		return nil, 0, dados.ErrInvalido{Motivo: "não consegui capturar o navegador: " + err.Error()}
	}
	if !anexar {
		s.registrarNavegador(context.WithoutCancel(ctx), "navegador.captura", tarefa, agente, map[string]any{"descricao": descricao})
		return captura, 0, nil
	}
	if legenda == "" {
		legenda = "Navegador: " + descricao
		if descricao == "" {
			legenda = "Navegador"
		}
		if r := []rune(legenda); len(r) > 200 {
			legenda = string(r[:199]) + "…"
		}
	}
	anexo, err := s.gravarAnexo(ctx, captura, "image/png", dados.NovoAnexo{Perfil: perfil, Tarefa: tarefa, Agente: agente, Origem: "captura",
		Legenda: legenda, Navegador: true})
	if err != nil {
		return nil, 0, err
	}
	return captura, anexo.ID, nil
}

// registrarNavegador grava um evento do navegador da tarefa.
func (s *Servidor) registrarNavegador(ctx context.Context, tipo string, tarefa, agente int64, conteudo map[string]any) {
	t, p, perfil, err := s.Banco.Tarefa(ctx, tarefa)
	if err != nil {
		return
	}
	conteudo["tarefa"], conteudo["titulo"], conteudo["projeto_nome"] = tarefa, t.Titulo, p.Nome
	escopo := dados.Escopo{Perfil: perfil, Projeto: p.ID, Tarefa: tarefa}
	if agente != 0 {
		escopo.Agente = agente
		conteudo["agente"] = agente
	}
	if err := s.Banco.Registrar(ctx, tipo, escopo, conteudo); err != nil {
		log.Printf("tarefa %d: gravando %s: %v", tarefa, tipo, err)
	}
}

// aoMudarNavegador recebe o fechamento das janelas (pelo usuário ou pelo fim do
// navegador). Grava na hora: o encerramento fecha os navegadores antes do banco.
func (s *Servidor) aoMudarNavegador(m navegador.Mudanca) {
	s.registrarNavegador(context.Background(), "navegador.fechado", m.Tarefa, 0, map[string]any{})
}
