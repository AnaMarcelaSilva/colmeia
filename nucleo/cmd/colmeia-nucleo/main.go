// Núcleo da Colmeia: roda em segundo plano e é dono dos terminais. As telas se
// conectam pelo canal local (socket Unix + token), nunca por porta de rede.
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/api"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/canal"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

const versao = "0.1.0"

func main() {
	quantidade := flag.Int("terminais", 10, "quantidade de terminais")
	modoDemo := flag.Bool("demo", false, "liga as cargas de teste (escrevem comandos nos terminais)")
	flag.Parse()
	if *quantidade < 1 || *quantidade > 64 {
		log.Fatal("--terminais precisa estar entre 1 e 64")
	}

	listener, token, fechar, err := canal.Abrir()
	if errors.Is(err, canal.ErrEmUso) {
		log.Fatal("já existe um núcleo da Colmeia rodando")
	}
	if err != nil {
		log.Fatalf("abrindo o canal local: %v", err)
	}
	defer fechar()

	var bytes atomic.Int64
	pasta, _ := os.UserHomeDir()
	sessoes := make([]*terminal.Sessao, 0, *quantidade)
	for i := range *quantidade {
		prompt := fmt.Sprintf("PS1=\\[\\e[35m\\]agente-%d\\[\\e[0m\\] \\w $ ", i)
		pty, err := terminal.Iniciar([]string{"bash", "--noprofile", "--norc"}, []string{prompt}, pasta)
		if err != nil {
			log.Fatalf("abrindo terminal %d: %v", i, err)
		}
		s := terminal.NovaSessao(i, pty, &bytes)
		sessoes = append(sessoes, s)
		go s.Ler()
	}

	servidor := &api.Servidor{Sessoes: sessoes, Bytes: &bytes, Versao: versao, Demo: *modoDemo}
	servidorHTTP := &http.Server{
		Handler:           canal.ExigirToken(token, servidor.Rotas()),
		ReadHeaderTimeout: 5 * time.Second,
		MaxHeaderBytes:    16 << 10,
	}

	parar := make(chan os.Signal, 1)
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
	for _, s := range sessoes {
		s.Fechar()
	}
}
