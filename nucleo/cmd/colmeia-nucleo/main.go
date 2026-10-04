// Núcleo da Colmeia: roda em segundo plano e é dono dos terminais. As telas se
// conectam pelo canal local (socket Unix + token), nunca por porta de rede.
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/api"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/canal"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/chaveiro"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/mcp"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

const versao = "0.2.0"

func main() {
	// "colmeia-nucleo mcp": o servidor MCP que o Claude Code de cada agente roda.
	if len(os.Args) > 1 && os.Args[1] == "mcp" {
		servirMCP(os.Args[2:])
		return
	}
	quantidade := flag.Int("terminais", 10, "quantidade de terminais de teste no modo demonstração")
	modoDemo := flag.Bool("demo", false, "modo demonstração: terminais de teste e cargas que escrevem comandos neles")
	encerrar := flag.Bool("encerrar", false, "encerra o núcleo que está rodando (e os agentes dele) e sai")
	mostrarVersao := flag.Bool("versao", false, "mostra a versão e sai")
	flag.Parse()
	if *mostrarVersao {
		fmt.Println("colmeia-nucleo", versao)
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
		pty, err := terminal.Iniciar([]string{"bash", "--noprofile", "--norc"}, []string{prompt}, pasta, terminal.TamanhoPadrao)
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
		Handler:           canal.Autenticar(token, fichas, servidor.Rotas()),
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
