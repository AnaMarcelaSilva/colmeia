// Núcleo da Colmeia: roda em segundo plano e é dono dos terminais. As telas se
// conectam pelo canal local (socket Unix + token), nunca por porta de rede.
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"runtime"
	"strings"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/api"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/atualizacao"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/canal"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/chaveiro"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/mcp"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/uso"
)

// versao é var para a release (e o teste da atualização) trocar com -ldflags -X main.versao=.
var versao = "0.4.0"

func main() {
	// "colmeia-nucleo mcp": o servidor MCP que o Claude Code de cada agente roda.
	if len(os.Args) > 1 && os.Args[1] == "mcp" {
		servirMCP(os.Args[2:])
		return
	}
	// "colmeia-nucleo uso": a statusLine do Claude Code de cada agente.
	if len(os.Args) > 1 && os.Args[1] == "uso" {
		mandarUso(os.Args[2:])
		return
	}
	quantidade := flag.Int("terminais", 10, "quantidade de terminais de teste no modo demonstração")
	modoDemo := flag.Bool("demo", false, "modo demonstração: terminais de teste e cargas que escrevem comandos neles")
	encerrar := flag.Bool("encerrar", false, "encerra o núcleo que está rodando (e os agentes dele) e sai")
	mostrarVersao := flag.Bool("versao", false, "mostra a versão e sai")
	// A troca de versão (veja internal/atualizacao): o núcleo que vai sair
	// abre a cópia dele com estes argumentos.
	trocar := flag.String("trocar", "", "troca os programas pela versão preparada nesta pasta e sai")
	esperar := flag.Int("esperar", 0, "com --trocar, o núcleo que precisa sair antes")
	abrirTela := flag.Bool("abrir-tela", false, "com --trocar, abre a tela no fim")
	flag.Parse()
	if *mostrarVersao {
		fmt.Println("colmeia-nucleo", versao)
		return
	}
	if *trocar != "" {
		if err := atualizacao.Trocar(*trocar, *esperar, *abrirTela); err != nil {
			log.Fatal(err)
		}
		return
	}
	if *encerrar {
		if err := pedirEncerramento(); err != nil {
			log.Fatal(err)
		}
		fmt.Println("núcleo encerrado")
		return
	}
	if *quantidade < 1 || *quantidade > 64 {
		log.Fatal("--terminais precisa estar entre 1 e 64")
	}
	// Fora da demonstração não há terminais de teste: os agentes de verdade
	// ganham terminal quando forem adicionados a uma tarefa.
	if !*modoDemo {
		*quantidade = 0
	}

	listener, token, fechar, err := canal.Abrir()
	if errors.Is(err, canal.ErrEmUso) {
		log.Fatal("já existe um núcleo da Colmeia rodando")
	}
	if err != nil {
		log.Fatalf("abrindo o canal local: %v", err)
	}
	defer fechar()

	dirDados, err := dados.Diretorio()
	if err != nil {
		log.Fatalf("sem diretório de dados: %v", err)
	}
	// O que o núcleo registra vai também para nucleo.log na pasta de dados:
	// aberto pela tela, o stderr dele é o nada, e sem isso um pedido lento
	// ou um erro some sem rastro.
	if registro := abrirRegistro(dirDados); registro != nil {
		defer registro.Close()
		log.SetOutput(io.MultiWriter(os.Stderr, registro))
	}
	banco, err := dados.Abrir(dirDados)
	if err != nil {
		log.Fatalf("abrindo os dados: %v", err)
	}
	defer banco.Fechar()
	avisoHistorico := ""
	if err := banco.VerificarHistorico(context.Background()); err != nil {
		// Não impede o uso, mas avisa (aqui e na tela): alguém mexeu no histórico por fora.
		log.Printf("atenção: %v", err)
		avisoHistorico = "O " + err.Error() + "."
	}
	banco.Registrar(context.Background(), "nucleo.iniciou", dados.Escopo{}, map[string]any{"versao": versao, "historico_ok": avisoHistorico == ""})

	var bytes atomic.Int64
	pasta, _ := os.UserHomeDir()
	sessoes := make([]*terminal.Sessao, 0, *quantidade)
	for i := range *quantidade {
		prompt := fmt.Sprintf("PS1=\\[\\e[35m\\]agente-%d\\[\\e[0m\\] \\w $ ", i)
		demo := []string{"bash", "--noprofile", "--norc"}
		if runtime.GOOS == "windows" {
			demo = []string{terminal.ShellPadrao()}
		}
		pty, err := terminal.Iniciar(demo, []string{prompt}, pasta, terminal.TamanhoPadrao)
		if err != nil {
			log.Fatalf("abrindo terminal %d: %v", i, err)
		}
		s := terminal.NovaSessao(int64(i), pty, &bytes)
		sessoes = append(sessoes, s)
		go s.Ler()
	}

	agentes := terminal.NovoGerente()
	parar := make(chan os.Signal, 1)
	dirCanal, _ := canal.Diretorio()
	api.LimparAgentesAntigos(dirCanal)
	executavel, err := os.Executable()
	if err != nil {
		log.Printf("sem o caminho do próprio núcleo; os agentes ficam sem as ferramentas da Colmeia: %v", err)
	}
	fichas := canal.NovasFichas()
	// Senhas das conexões de banco: o chaveiro do sistema, sondado já (a
	// primeira resposta do D-Bus pode demorar), ou só a memória.
	cofre := chaveiro.DoAmbiente()
	go cofre.Disponivel()
	servidor := &api.Servidor{Chaveiro: cofre, Sessoes: sessoes, Agentes: agentes, Bytes: &bytes, Versao: versao, Demo: *modoDemo, Banco: banco, DirDados: dirDados,
		AvisoHistorico: avisoHistorico, AoEncerrar: func() { parar <- syscall.SIGTERM },
		Fichas: fichas, DirCanal: dirCanal, Executavel: executavel}
	servidorHTTP := &http.Server{
		Handler:           canal.Autenticar(token, fichas, registrarLentos(servidor.Rotas())),
		ReadHeaderTimeout: 5 * time.Second,
		MaxHeaderBytes:    16 << 10,
	}

	signal.Notify(parar, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-parar
		ctx, cancelar := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancelar()
		servidorHTTP.Shutdown(ctx)
	}()

	log.Printf("núcleo %s com %d terminais no canal %s (demo: %v)", versao, *quantidade, listener.Addr(), *modoDemo)
	if err := servidorHTTP.Serve(listener); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Printf("servidor: %v", err)
	}
	// Fechar o terminal avisa cada agente, que tem uns segundos para salvar a
	// conversa; o fim de cada um é gravado antes de o banco fechar.
	agentes.FecharTodos()
	servidor.Execucoes.FecharTodos()
	servidor.Navegadores.FecharTodos()
	servidor.Encerrar()
	for _, s := range sessoes {
		s.Fechar()
	}
}

// servirMCP atende o Claude Code de um agente pelo stdin e stdout. O token
// do agente vem de um arquivo 0600 (nunca de argumento ou variável de
// ambiente), e o log vai só para o stderr: o stdout é do protocolo.
func servirMCP(args []string) {
	log.SetOutput(os.Stderr)
	opcoes := flag.NewFlagSet("mcp", flag.ExitOnError)
	socket := opcoes.String("socket", "", "socket do núcleo")
	arquivoToken := opcoes.String("token-arquivo", "", "arquivo com o token do agente")
	opcoes.Parse(args)
	if *socket == "" || *arquivoToken == "" {
		log.Fatal("uso: colmeia-nucleo mcp --socket <nucleo.sock> --token-arquivo <arquivo>")
	}
	token, err := os.ReadFile(*arquivoToken)
	if err != nil {
		log.Fatalf("lendo o token do agente: %v", err)
	}
	s := &mcp.Servidor{Nucleo: mcp.NovoClienteSocket(*socket, strings.TrimSpace(string(token))), Versao: versao}
	if err := s.Servir(context.Background(), os.Stdin, os.Stdout); err != nil {
		log.Fatal(err)
	}
}

// mandarUso lê o JSON da statusLine no stdin, manda ao núcleo só o contexto
// e os limites (com o token do agente, lido do arquivo 0600) e imprime uma
// linha curta. Roda a cada atualização da statusLine: nada de log, prazo
// curto, e a linha sai mesmo se o núcleo não responder.
func mandarUso(args []string) {
	opcoes := flag.NewFlagSet("uso", flag.ContinueOnError)
	opcoes.SetOutput(io.Discard)
	socket := opcoes.String("socket", "", "socket do núcleo")
	arquivoToken := opcoes.String("token-arquivo", "", "arquivo com o token do agente")
	if opcoes.Parse(args) != nil || *socket == "" || *arquivoToken == "" {
		fmt.Println("Colmeia")
		return
	}
	u, err := uso.Ler(os.Stdin)
	if err != nil {
		fmt.Println("Colmeia")
		return
	}
	fmt.Println(u.Linha())
	token, err := os.ReadFile(*arquivoToken)
	if err != nil {
		return
	}
	ctx, cancelar := context.WithTimeout(context.Background(), 500*time.Millisecond)
	defer cancelar()
	mcp.NovoClienteSocket(*socket, strings.TrimSpace(string(token))).Pedir(ctx, "POST", "/v1/agente/uso", u)
}

// pedirEncerramento fala com o núcleo que está rodando pelo canal local (o
// mesmo socket e token da tela) e pede para ele desligar.
func pedirEncerramento() error {
	dir, err := canal.Diretorio()
	if err != nil {
		return err
	}
	token, err := os.ReadFile(filepath.Join(dir, canal.NomeToken))
	if err != nil {
		return fmt.Errorf("nenhum núcleo rodando (sem token em %s)", dir)
	}
	cliente := &http.Client{
		Timeout: 5 * time.Second,
		Transport: &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
			var d net.Dialer
			return d.DialContext(ctx, "unix", filepath.Join(dir, canal.NomeSocket))
		}},
	}
	pedido, _ := http.NewRequest("POST", "http://colmeia/v1/encerrar", nil)
	pedido.Header.Set("Authorization", "Bearer "+strings.TrimSpace(string(token)))
	resposta, err := cliente.Do(pedido)
	if err != nil {
		return fmt.Errorf("nenhum núcleo respondeu: %w", err)
	}
	resposta.Body.Close()
	if resposta.StatusCode != http.StatusAccepted {
		return fmt.Errorf("o núcleo recusou: %s", resposta.Status)
	}
	// Espera o socket sumir: o núcleo encerrou os agentes e fechou o banco.
	for range 100 {
		if _, err := os.Stat(filepath.Join(dir, canal.NomeSocket)); os.IsNotExist(err) {
			return nil
		}
		time.Sleep(100 * time.Millisecond)
	}
	return nil
}

// abrirRegistro abre nucleo.log (0600) na pasta de dados; passando de 2 MB,
// o anterior vira nucleo.log.1.
func abrirRegistro(dir string) *os.File {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil
	}
	caminho := filepath.Join(dir, "nucleo.log")
	if info, err := os.Stat(caminho); err == nil && info.Size() > 2<<20 {
		os.Rename(caminho, caminho+".1")
	}
	f, err := os.OpenFile(caminho, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return nil
	}
	return f
}

// pedidoLento é a partir de quanto um pedido entra no log.
const pedidoLento = 3 * time.Second

// registrarLentos anota no log os pedidos que passam de pedidoLento: quando
// a tela reclama que o núcleo não respondeu, aqui fica dito qual pedido
// travou. Os que duram de propósito (terminais, eventos e as esperas do
// agente) ficam de fora.
func registrarLentos(proximo http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		inicio := time.Now()
		proximo.ServeHTTP(w, r)
		if d := time.Since(inicio); d > pedidoLento && !demoraDeProposito(r.URL.Path) {
			log.Printf("pedido lento: %s %s levou %s", r.Method, r.URL.Path, d.Round(100*time.Millisecond))
		}
	})
}

// demoraDeProposito: os caminhos que ficam abertos esperando (WebSocket,
// eventos, o agente esperando a aprovação ou a resposta).
func demoraDeProposito(caminho string) bool {
	return strings.Contains(caminho, "/termina") || strings.HasSuffix(caminho, "/eventos") || strings.HasPrefix(caminho, "/v1/agente/")
}
