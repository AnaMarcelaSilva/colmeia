package api

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"log"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/bancos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/chaveiro"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// Conexões de banco por perfil: criar, testar, navegar, consultar e alterar
// (com confirmação). A senha entra por aqui e vai para o chaveiro do sistema
// ou só para a memória; nunca para o banco da Colmeia, eventos, avisos ou
// respostas.

const (
	// Maior SQL aceito.
	limiteSQL = 1 << 20
	// PrazoConfirmacao de uma alteração: a confirmação vale por isso, uma vez.
	PrazoConfirmacao = 2 * time.Minute
	// Tempo-limite máximo de uma execução.
	maxTempoExecucao = 600
)

// cofre guarda as senhas que estão na memória do núcleo (as digitadas sem
// chaveiro e as lidas dele, para não perguntar ao D-Bus a cada consulta) e
// as confirmações de alteração pendentes.
type cofre struct {
	mu           sync.Mutex
	senhas       map[int64]string
	confirmacoes map[string]confirmacao
}

type confirmacao struct {
	hash   string
	expira time.Time
}

func (s *Servidor) iniciarBancos() {
	if s.Bancos == nil {
		s.Bancos = bancos.NovoGerente()
	}
	if s.Chaveiro == nil {
		s.Chaveiro = chaveiro.NovaMemoria()
	}
	if s.PrazoAprovacao == 0 {
		s.PrazoAprovacao = PrazoAprovacaoPadrao
	}
	s.cofre = &cofre{senhas: map[int64]string{}, confirmacoes: map[string]confirmacao{}}
	s.aprovacoes = map[string]*aprovacao{}
}

func (s *Servidor) rotasBancos(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/perfis/{id}/conexoes", s.listarConexoes)
	mux.HandleFunc("POST /v1/perfis/{id}/conexoes", s.criarConexao)
	mux.HandleFunc("POST /v1/perfis/{id}/conexoes/testar", s.testarRascunho)
	mux.HandleFunc("GET /v1/conexoes/{id}", s.lerConexao)
	mux.HandleFunc("PATCH /v1/conexoes/{id}", s.editarConexao)
	mux.HandleFunc("DELETE /v1/conexoes/{id}", s.removerConexao)
	mux.HandleFunc("PUT /v1/conexoes/{id}/senha", s.definirSenha)
	mux.HandleFunc("DELETE /v1/conexoes/{id}/senha", s.esquecerSenha)
	mux.HandleFunc("POST /v1/conexoes/{id}/testar", s.testarConexao)
	mux.HandleFunc("POST /v1/conexoes/{id}/desconectar", s.desconectar)
	mux.HandleFunc("GET /v1/conexoes/{id}/arvore", s.arvore)
	mux.HandleFunc("POST /v1/conexoes/{id}/previa", s.previa)
	mux.HandleFunc("POST /v1/conexoes/{id}/execucoes", s.executar)
	mux.HandleFunc("GET /v1/execucoes/{ficha}/mais", s.carregarMais)
	mux.HandleFunc("DELETE /v1/execucoes/{ficha}", s.cancelarExecucao)
	mux.HandleFunc("GET /v1/conexoes/{id}/historico", s.historico)
	mux.HandleFunc("DELETE /v1/conexoes/{id}/historico", s.limparHistorico)
	mux.HandleFunc("GET /v1/perfis/{id}/aprovacoes", s.listarAprovacoes)
	mux.HandleFunc("POST /v1/aprovacoes/{id}", s.responderAprovacao)
}

// Senhas

func (s *Servidor) senhaNaMemoria(conexao int64) (string, bool) {
	s.cofre.mu.Lock()
	defer s.cofre.mu.Unlock()
	v, ok := s.cofre.senhas[conexao]
	return v, ok
}

func (s *Servidor) lembrarSenha(conexao int64, senha string) {
	s.cofre.mu.Lock()
	defer s.cofre.mu.Unlock()
	s.cofre.senhas[conexao] = senha
}

func (s *Servidor) esquecerDaMemoria(conexao int64) {
	s.cofre.mu.Lock()
	defer s.cofre.mu.Unlock()
	delete(s.cofre.senhas, conexao)
}

// errSemSenha: a conexão precisa de senha e ela não está na memória (sem
// chaveiro, ou depois de reiniciar o núcleo).
var errSemSenha = errors.New("digite a senha da conexão")

// senhaDe acha a senha da conexão: na memória, no chaveiro ou nenhuma.
func (s *Servidor) senhaDe(c dados.ConexaoBanco) (string, error) {
	if c.Senha == "nenhuma" {
		return "", nil
	}
	if v, ok := s.senhaNaMemoria(c.ID); ok {
		return v, nil
	}
	if c.Senha == "memoria" {
		return "", errSemSenha
	}
	v, err := s.Chaveiro.Ler(c.ChaveSegredo)
	switch {
	case err == nil:
		s.lembrarSenha(c.ID, v)
		return v, nil
	case errors.Is(err, chaveiro.ErrBloqueado):
		return "", dados.ErrInvalido{Motivo: "O chaveiro está bloqueado: desbloqueie ou digite a senha."}
	}
	return "", errSemSenha
}

// guardarSenha põe a senha no chaveiro (se pedido e se houver) ou só na
// memória, e devolve onde ela mora. Senha vazia: a conexão não usa senha.
//
// A falta do chaveiro (fora do ar, bloqueado, COLMEIA_CHAVEIRO=memoria) é
// estado da sessão, não da conexão: uma conexão do chaveiro continua marcada
// "chaveiro" e a senha fica só na memória até o núcleo encerrar. Quando o
// chaveiro volta, a Colmeia procura nele de novo (e, se não achar, pergunta
// e guarda lá). Só "memoria" quando você desmarca "Guardar no chaveiro" com
// ele disponível.
func (s *Servidor) guardarSenha(ctx context.Context, c dados.ConexaoBanco, senha string, noChaveiro bool) (string, error) {
	disponivel := s.Chaveiro.Disponivel()
	onde := "memoria"
	switch {
	case senha == "":
		onde = "nenhuma"
		if disponivel {
			s.Chaveiro.Apagar(c.ChaveSegredo)
		}
		s.esquecerDaMemoria(c.ID)
	case noChaveiro || (!disponivel && c.Senha == "chaveiro"):
		onde = "chaveiro"
		if disponivel {
			if err := s.Chaveiro.Guardar(c.ChaveSegredo, senha); err != nil {
				log.Printf("conexão %d: o chaveiro não guardou a senha; nesta sessão ela fica só na memória", c.ID)
			}
		}
		s.lembrarSenha(c.ID, senha)
	default:
		s.Chaveiro.Apagar(c.ChaveSegredo)
		s.lembrarSenha(c.ID, senha)
	}
	s.Bancos.FecharConexao(c.ID)
	return onde, s.Banco.DefinirOndeSenha(ctx, c.ID, onde)
}

// config monta o que o pacote bancos precisa, com a senha.
func config(c dados.ConexaoBanco, senha string) bancos.Config {
	return bancos.Config{ID: c.ID, Tipo: c.Tipo, Host: c.Host, Porta: c.Porta, Usuario: c.Usuario, Senha: senha, Banco: c.Banco,
		Arquivo: c.Arquivo, SSL: c.SSL, SSLCA: c.SSLCA, Escrita: c.Escrita}
}

// conexaoComSenha lê a conexão da rota e a senha dela.
func (s *Servidor) conexaoComSenha(r *http.Request) (dados.ConexaoBanco, bancos.Config, error) {
	id, err := idDaRota(r)
	if err != nil {
		return dados.ConexaoBanco{}, bancos.Config{}, err
	}
	c, err := s.Banco.ConexaoBanco(r.Context(), id)
	if err != nil {
		return c, bancos.Config{}, err
	}
	senha, err := s.senhaDe(c)
	if err != nil {
		return c, bancos.Config{}, err
	}
	return c, config(c, senha), nil
}

// Respostas

// conexaoParaTela: a conexão, se a senha está à mão agora e se há chaveiro.
type conexaoParaTela struct {
	dados.ConexaoBanco
	SenhaDisponivel bool `json:"senha_disponivel"`
}

func (s *Servidor) paraTela(c dados.ConexaoBanco) conexaoParaTela {
	_, naMemoria := s.senhaNaMemoria(c.ID)
	disponivel := c.Senha == "nenhuma" || naMemoria || (c.Senha == "chaveiro" && s.Chaveiro.Disponivel())
	return conexaoParaTela{ConexaoBanco: c, SenhaDisponivel: disponivel}
}

func responderStatus(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(v)
}

// responderErroBanco: a senha que falta (428), somente leitura (403) e os
// erros do servidor de banco (422, com senha_recusada quando o servidor
// recusou usuário ou senha), em frases simples, com o detalhe já sem a senha.
func responderErroBanco(w http.ResponseWriter, err error, c bancos.Config, tempo time.Duration) {
	var invalido dados.ErrInvalido
	switch {
	case errors.Is(err, errSemSenha):
		responderStatus(w, http.StatusPreconditionRequired, map[string]any{"erro": err.Error(), "precisa_senha": true})
	case errors.As(err, &invalido), errors.Is(err, dados.ErrNaoEncontrado):
		responderErro(w, err)
	case errors.Is(err, bancos.ErrSomenteLeitura):
		responderStatus(w, http.StatusForbidden, map[string]any{"erro": err.Error(), "somente_leitura": true})
	case errors.Is(err, bancos.ErrTempoEsgotado):
		responderStatus(w, http.StatusUnprocessableEntity, map[string]any{"erro": "Passou de " + segundos(tempo) + " e foi cancelada.", "tempo_esgotado": true})
	case errors.Is(err, bancos.ErrCancelada):
		responderStatus(w, http.StatusUnprocessableEntity, map[string]any{"erro": "Consulta cancelada.", "cancelada": true})
	case errors.Is(err, bancos.ErrSemExecucao), errors.Is(err, bancos.ErrFechada):
		responderStatus(w, http.StatusGone, map[string]any{"erro": err.Error(), "fechada": true})
	case errors.Is(err, bancos.ErrVarias), errors.Is(err, bancos.ErrVazia), errors.Is(err, bancos.ErrFichaEmUso):
		responderStatus(w, http.StatusBadRequest, map[string]any{"erro": err.Error(), "varias": errors.Is(err, bancos.ErrVarias)})
	default:
		frase, detalhe := bancos.Explicar(err, c)
		resposta := map[string]any{"erro": frase, "detalhe": detalhe}
		if bancos.SenhaRecusada(err) {
			// A senha guardada ficou errada (trocada no servidor) ou a conexão
			// foi salva sem senha: a tela oferece trocar a senha ali mesmo.
			resposta["senha_recusada"] = true
		}
		var eb bancos.ErrBanco
		if errors.As(err, &eb) && frase == "Não conectou." {
			// Erro do SQL (não de conexão): a mensagem do servidor é o que importa.
			resposta = map[string]any{"erro": eb.Mensagem, "do_servidor": true}
			if eb.Linha > 0 {
				resposta["linha"] = eb.Linha
			}
		}
		responderStatus(w, http.StatusUnprocessableEntity, resposta)
	}
}

func segundos(d time.Duration) string {
	if d >= time.Minute && d%time.Minute == 0 {
		return strconv.Itoa(int(d/time.Minute)) + " min"
	}
	return strconv.Itoa(int(d.Seconds())) + " s"
}

// nomeDeBancoValido: o banco escolhido na barra do console (vazio é o padrão).
func nomeDeBancoValido(nome string) error {
	if len(nome) > 128 || strings.ContainsFunc(nome, func(r rune) bool { return r < ' ' || r == 0x7f }) {
		return dados.ErrInvalido{Motivo: "nome de banco inválido"}
	}
	return nil
}

// Conexões

func (s *Servidor) listarConexoes(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarConexoes(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	conexoes := make([]conexaoParaTela, len(lista))
	for i, c := range lista {
		conexoes[i] = s.paraTela(c)
	}
	responderJSON(w, map[string]any{"conexoes": conexoes, "chaveiro_disponivel": s.Chaveiro.Disponivel()})
}

// pedidoConexao: os campos, a senha (só escrita, nunca devolvida) e se ela
// vai para o chaveiro.
type pedidoConexao struct {
	dados.CamposConexao
	Senha   *string `json:"senha"`
	Guardar bool    `json:"guardar"`
}

func (s *Servidor) criarConexao(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido pedidoConexao
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	senha := ""
	if pedido.Senha != nil && pedido.Tipo != "sqlite" {
		senha = *pedido.Senha
	}
	c, err := s.Banco.CriarConexao(r.Context(), perfil, pedido.CamposConexao, "nenhuma")
	if err != nil {
		responderErro(w, err)
		return
	}
	if senha != "" {
		if c.Senha, err = s.guardarSenha(context.WithoutCancel(r.Context()), c, senha, pedido.Guardar); err != nil {
			responderErro(w, err)
			return
		}
	}
	responderJSON(w, s.paraTela(c))
}

func (s *Servidor) lerConexao(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.ConexaoBanco(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.paraTela(c))
}

// editarConexao troca os campos; senha vazia ou ausente não muda a senha.
func (s *Servidor) editarConexao(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido pedidoConexao
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.AtualizarConexao(r.Context(), id, pedido.CamposConexao)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.Bancos.FecharConexao(id)
	if pedido.Senha != nil && *pedido.Senha != "" && c.Tipo != "sqlite" {
		if c.Senha, err = s.guardarSenha(context.WithoutCancel(r.Context()), c, *pedido.Senha, pedido.Guardar); err != nil {
			responderErro(w, err)
			return
		}
	}
	responderJSON(w, s.paraTela(c))
}

func (s *Servidor) removerConexao(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.Bancos.FecharConexao(id)
	c, err := s.Banco.RemoverConexao(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.esquecerDaMemoria(id)
	if err := s.Chaveiro.Apagar(c.ChaveSegredo); err != nil {
		log.Printf("conexão %d: a senha não saiu do chaveiro: %v", id, err)
	}
	responderJSON(w, map[string]any{"ok": true})
}

func (s *Servidor) definirSenha(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Senha   string `json:"senha"`
		Guardar bool   `json:"guardar"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.ConexaoBanco(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	onde, err := s.guardarSenha(r.Context(), c, pedido.Senha, pedido.Guardar)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"senha": onde})
}

// esquecerSenha tira a senha do chaveiro e da memória: a próxima conexão pergunta.
func (s *Servidor) esquecerSenha(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.ConexaoBanco(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.Chaveiro.Apagar(c.ChaveSegredo)
	s.esquecerDaMemoria(id)
	s.Bancos.FecharConexao(id)
	onde := "memoria"
	if c.Tipo == "sqlite" {
		onde = "nenhuma"
	}
	if err := s.Banco.DefinirOndeSenha(r.Context(), id, onde); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"senha": onde})
}

func responderTeste(w http.ResponseWriter, info bancos.InfoTeste, err error, c bancos.Config) {
	if err != nil {
		frase, detalhe := bancos.Explicar(err, c)
		responderJSON(w, map[string]any{"ok": false, "erro": frase, "detalhe": detalhe})
		return
	}
	responderJSON(w, map[string]any{"ok": true, "ms": info.Ms, "servidor": info.Servidor, "tls": info.TLS})
}

// testarRascunho testa os campos do diálogo antes de salvar. Sem senha e com
// conexao_id (editando), usa a senha salva.
func (s *Servidor) testarRascunho(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		dados.CamposConexao
		Senha     string `json:"senha"`
		ConexaoID int64  `json:"conexao_id"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	campos, err := dados.ValidarConexao(pedido.CamposConexao)
	if err != nil {
		responderErro(w, err)
		return
	}
	senha := pedido.Senha
	if senha == "" && pedido.ConexaoID != 0 {
		salva, err := s.Banco.ConexaoBanco(r.Context(), pedido.ConexaoID)
		if err != nil || salva.PerfilID != perfil {
			responderErro(w, dados.ErrNaoEncontrado)
			return
		}
		if senha, err = s.senhaDe(salva); err != nil && !errors.Is(err, errSemSenha) {
			responderErro(w, err)
			return
		}
	}
	c := config(dados.ConexaoBanco{Tipo: campos.Tipo, Host: campos.Host, Porta: campos.Porta, Usuario: campos.Usuario, Banco: campos.Banco,
		Arquivo: campos.Arquivo, SSL: campos.SSL, SSLCA: campos.SSLCA}, senha)
	info, err := bancos.Testar(r.Context(), c)
	responderTeste(w, info, err, c)
}

func (s *Servidor) testarConexao(w http.ResponseWriter, r *http.Request) {
	_, c, err := s.conexaoComSenha(r)
	if err != nil {
		responderErroBanco(w, err, c, 0)
		return
	}
	info, err := bancos.Testar(r.Context(), c)
	responderTeste(w, info, err, c)
}

func (s *Servidor) desconectar(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.Bancos.FecharConexao(id)
	responderJSON(w, map[string]any{"ok": true})
}

// Árvore e prévia

// arvore devolve um nível por vez: bancos, esquemas, objetos (tabelas e
// views) ou colunas. Filtro e contagem ficam na tela.
func (s *Servidor) arvore(w http.ResponseWriter, r *http.Request) {
	_, c, err := s.conexaoComSenha(r)
	if err != nil {
		responderErroBanco(w, err, c, 0)
		return
	}
	q := r.URL.Query()
	banco, esquema, objeto := q.Get("banco"), q.Get("esquema"), q.Get("objeto")
	ctx, cancelar := context.WithTimeout(r.Context(), 60*time.Second)
	defer cancelar()
	switch q.Get("nivel") {
	case "bancos":
		nomes, err := s.Bancos.Bancos(ctx, c)
		if err != nil {
			responderErroBanco(w, err, c, 0)
			return
		}
		responderJSON(w, map[string]any{"nomes": nomes, "padrao": c.Banco})
	case "esquemas":
		nomes, err := s.Bancos.Esquemas(ctx, c, banco)
		if err != nil {
			responderErroBanco(w, err, c, 0)
			return
		}
		responderJSON(w, map[string]any{"nomes": nomes})
	case "objetos":
		objetos, total, err := s.Bancos.Objetos(ctx, c, banco, esquema)
		if err != nil {
			responderErroBanco(w, err, c, 0)
			return
		}
		tabelas, views := []string{}, []string{}
		for _, o := range objetos {
			if o.View {
				views = append(views, o.Nome)
			} else {
				tabelas = append(tabelas, o.Nome)
			}
		}
		responderJSON(w, map[string]any{"tabelas": tabelas, "views": views, "total": total, "cortado": total > len(objetos)})
	case "colunas":
		colunas, err := s.Bancos.Colunas(ctx, c, banco, esquema, objeto)
		if err != nil {
			responderErroBanco(w, err, c, 0)
			return
		}
		responderJSON(w, map[string]any{"colunas": colunas})
	default:
		responderErro(w, dados.ErrInvalido{Motivo: "nível inválido: use bancos, esquemas, objetos ou colunas"})
	}
}

func (s *Servidor) previa(w http.ResponseWriter, r *http.Request) {
	conexao, c, err := s.conexaoComSenha(r)
	if err != nil {
		responderErroBanco(w, err, c, 0)
		return
	}
	var pedido struct {
		Banco   string `json:"banco"`
		Esquema string `json:"esquema"`
		Objeto  string `json:"objeto"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Objeto == "" {
		responderErro(w, dados.ErrInvalido{Motivo: "informe a tabela"})
		return
	}
	resultado, sql, err := s.Bancos.Previa(r.Context(), c, pedido.Banco, pedido.Esquema, pedido.Objeto, 100)
	s.registrarConsulta(r.Context(), conexao, consultaFeita{verbo: "SELECT", linhas: int64(len(resultado.Linhas)), ms: resultado.Ms, erro: err != nil, previa: true})
	if err != nil {
		responderErroBanco(w, err, c, 30*time.Second)
		return
	}
	responderJSON(w, map[string]any{"resultado": resultado, "sql": sql})
}

// Execução

// hashConfirmacao prende a confirmação à conexão, ao banco e ao SQL exatos.
func hashConfirmacao(conexao int64, banco, sql string) string {
	h := sha256.Sum256([]byte(strconv.FormatInt(conexao, 10) + "\x00" + banco + "\x00" + sql))
	return hex.EncodeToString(h[:])
}

// novaConfirmacao guarda o nonce (de uso único, por PrazoConfirmacao).
func (s *Servidor) novaConfirmacao(hash string) string {
	var b [24]byte
	rand.Read(b[:])
	nonce := hex.EncodeToString(b[:])
	agora := time.Now()
	s.cofre.mu.Lock()
	defer s.cofre.mu.Unlock()
	// As vencidas saem quando outra entra (nada de varredura).
	for n, c := range s.cofre.confirmacoes {
		if agora.After(c.expira) {
			delete(s.cofre.confirmacoes, n)
		}
	}
	s.cofre.confirmacoes[nonce] = confirmacao{hash: hash, expira: agora.Add(PrazoConfirmacao)}
	return nonce
}

// usarConfirmacao confere e gasta o nonce.
func (s *Servidor) usarConfirmacao(nonce, hash string) bool {
	s.cofre.mu.Lock()
	defer s.cofre.mu.Unlock()
	c, ok := s.cofre.confirmacoes[nonce]
	delete(s.cofre.confirmacoes, nonce)
	return ok && time.Now().Before(c.expira) && c.hash == hash
}

func (s *Servidor) executar(w http.ResponseWriter, r *http.Request) {
	conexao, c, err := s.conexaoComSenha(r)
	if err != nil {
		responderErroBanco(w, err, c, 0)
		return
	}
	var pedido struct {
		Ficha     string `json:"ficha"`
		SQL       string `json:"sql"`
		Banco     string `json:"banco"`
		Limite    int    `json:"limite"`
		TempoS    int    `json:"tempo_s"`
		Confirmar string `json:"confirmar"`
	}
	if err := lerAte(r, &pedido, limiteSQL+4096); err != nil {
		responderErro(w, err)
		return
	}
	if !bancos.FichaValida(pedido.Ficha) {
		responderErro(w, dados.ErrInvalido{Motivo: "ficha inválida"})
		return
	}
	if err := nomeDeBancoValido(pedido.Banco); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Limite == 0 {
		pedido.Limite = 500
	}
	if pedido.TempoS == 0 {
		pedido.TempoS = 30
	}
	if pedido.Limite < 1 || pedido.Limite > bancos.MaxLinhasPagina || pedido.TempoS < 1 || pedido.TempoS > maxTempoExecucao {
		responderErro(w, dados.ErrInvalido{Motivo: "limite de 1 a 5000 linhas e tempo de 1 a 600 s"})
		return
	}
	cl, err := bancos.Classificar(c.Tipo, pedido.SQL)
	if err != nil {
		responderErroBanco(w, err, c, 0)
		return
	}
	tempo := time.Duration(pedido.TempoS) * time.Second
	confirmada := false
	if cl.Classe == bancos.Altera {
		if !c.Escrita {
			responderErroBanco(w, bancos.ErrSomenteLeitura, c, 0)
			return
		}
		hash := hashConfirmacao(conexao.ID, pedido.Banco, pedido.SQL)
		if pedido.Confirmar == "" {
			responderStatus(w, http.StatusConflict, map[string]any{"precisa_confirmar": true, "verbo": cl.Verbo, "sql": pedido.SQL,
				"banco": pedido.Banco, "sem_where": cl.SemWhere, "confirmacao": s.novaConfirmacao(hash)})
			return
		}
		if !s.usarConfirmacao(pedido.Confirmar, hash) {
			responderStatus(w, http.StatusConflict, map[string]any{"erro": "A confirmação não vale mais (já usada, vencida ou de outra instrução). Execute de novo.",
				"confirmacao_invalida": true})
			return
		}
		confirmada = true
	}
	resultado, err := s.Bancos.Executar(r.Context(), c, bancos.Opcoes{Ficha: pedido.Ficha, SQL: pedido.SQL, Banco: pedido.Banco,
		Limite: pedido.Limite, Tempo: tempo, Confirmada: confirmada})
	feita := consultaFeita{sql: pedido.SQL, banco: pedido.Banco, verbo: cl.Verbo, linhas: int64(len(resultado.Linhas)), ms: resultado.Ms,
		erro: err != nil, altera: cl.Classe == bancos.Altera}
	if resultado.Afetadas != nil {
		feita.linhas = *resultado.Afetadas
	}
	if errors.Is(err, bancos.ErrCancelada) {
		// No histórico, "Cancelada", não "Erro".
		feita.resultado = cancelada
	}
	s.registrarConsulta(r.Context(), conexao, feita)
	if err != nil {
		responderErroBanco(w, err, c, tempo)
		return
	}
	responderJSON(w, resultado)
}

func (s *Servidor) carregarMais(w http.ResponseWriter, r *http.Request) {
	ficha := r.PathValue("ficha")
	if !bancos.FichaValida(ficha) {
		responderErro(w, dados.ErrInvalido{Motivo: "ficha inválida"})
		return
	}
	limite, _ := strconv.Atoi(r.URL.Query().Get("limite"))
	resultado, err := s.Bancos.Mais(r.Context(), ficha, limite)
	if err != nil {
		responderErroBanco(w, err, bancos.Config{}, 0)
		return
	}
	responderJSON(w, resultado)
}

func (s *Servidor) cancelarExecucao(w http.ResponseWriter, r *http.Request) {
	ficha := r.PathValue("ficha")
	if !bancos.FichaValida(ficha) {
		responderErro(w, dados.ErrInvalido{Motivo: "ficha inválida"})
		return
	}
	responderJSON(w, map[string]any{"ok": s.Bancos.Cancelar(ficha)})
}

// Histórico

func (s *Servidor) historico(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarConsultas(r.Context(), id, 200)
	if err != nil {
		responderErro(w, err)
		return
	}
	hoje := time.Now().Format("2006-01-02")
	for i, c := range lista {
		if t, err := time.Parse(time.RFC3339Nano, c.Momento); err == nil {
			if t.Local().Format("2006-01-02") == hoje {
				lista[i].Hora = hora(t)
			} else {
				lista[i].Hora = t.Local().Format("02/01")
			}
		}
	}
	responderJSON(w, map[string]any{"consultas": lista})
}

func (s *Servidor) limparHistorico(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if err := s.Banco.LimparConsultas(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}

// Registro: o histórico (com o SQL, apagável) e o evento (sem SQL nem resultado).

type consultaFeita struct {
	sql, banco, verbo string
	linhas, ms        int64
	erro, altera      bool
	previa            bool
	// Pedidos do agente.
	agente    dados.ContextoAgente
	resultado string // aprovada, recusada, expirou, cancelada
}

func (s *Servidor) registrarConsulta(ctx context.Context, c dados.ConexaoBanco, f consultaFeita) {
	ctx = context.WithoutCancel(ctx)
	origem := dados.OrigemVoce
	if f.agente.ID != 0 {
		origem = "agente"
	}
	if f.sql != "" {
		if _, err := s.Banco.GuardarConsulta(ctx, dados.ConsultaBanco{ConexaoID: c.ID, SQL: f.sql, Banco: f.banco, Origem: origem, AgenteID: f.agente.ID,
			DuracaoMs: f.ms, Linhas: f.linhas, Erro: f.erro, Altera: f.altera, Resultado: f.resultado}); err != nil {
			log.Printf("conexão %d: guardando o histórico: %v", c.ID, err)
		}
	}
	conteudo := map[string]any{"conexao_id": c.ID, "conexao": c.Nome, "tipo": c.Tipo, "verbo": f.verbo, "linhas": f.linhas, "ms": f.ms,
		"erro": f.erro, "origem": origem}
	if f.previa {
		conteudo["previa"] = true
	}
	escopo := dados.Escopo{Perfil: c.PerfilID}
	if f.agente.ID != 0 {
		escopo = f.agente.Escopo()
		conteudo["resultado"], conteudo["agente"] = f.resultado, f.agente.ID
		conteudo["ferramenta"], conteudo["papel"], conteudo["titulo"], conteudo["projeto_nome"] = f.agente.Ferramenta, f.agente.Papel, f.agente.Titulo, f.agente.ProjetoNome
	}
	tipo := "banco.consulta"
	if f.altera {
		tipo = "banco.alteracao"
		conteudo["linhas_afetadas"] = f.linhas
		delete(conteudo, "linhas")
	}
	if err := s.Banco.Registrar(ctx, tipo, escopo, conteudo); err != nil {
		log.Printf("conexão %d: gravando %s: %v", c.ID, tipo, err)
	}
}

// textoTabela monta o resultado para o agente: uma linha por registro,
// colunas separadas por " | ", até `limite` bytes.
func textoTabela(r bancos.Resultado, limite int) (string, bool) {
	var b strings.Builder
	nomes := make([]string, len(r.Colunas))
	for i, c := range r.Colunas {
		nomes[i] = c.Nome
	}
	b.WriteString(strings.Join(nomes, " | "))
	b.WriteByte('\n')
	for _, linha := range r.Linhas {
		celulas := make([]string, len(linha))
		for i, v := range linha {
			if v == nil {
				celulas[i] = "NULL"
			} else {
				celulas[i] = strings.NewReplacer("\n", "\\n", "\r", "", "|", "\\|").Replace(*v)
			}
		}
		texto := strings.Join(celulas, " | ") + "\n"
		if b.Len()+len(texto) > limite {
			return b.String(), true
		}
		b.WriteString(texto)
	}
	return b.String(), false
}
