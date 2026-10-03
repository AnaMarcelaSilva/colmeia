package segredos

import "testing"

func TestParece(t *testing.T) {
	casos := []struct {
		texto  string
		parece bool
	}{
		{"use a chave sk-ant-api03-AbCdEfGhIjKlMnOpQrStUv para testar", true},
		{"OPENAI_KEY sk-proj-abcdefghijklmnopqrstuvwx", true},
		{"ghp_0123456789abcdefghijABCDEFGHIJ", true},
		{"gho_0123456789abcdefghijABCDEFGHIJ", true},
		{"github_pat_11ABCDEFG0123456789_abcdefghij", true},
		{"glpat-abcdefghij0123456789", true},
		{"xoxb-123456789012-abcdefghij", true},
		{"AKIAIOSFODNN7EXAMPLE", true},
		{"-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk=", true},
		{"-----BEGIN PRIVATE KEY-----", true},
		{"Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.assinatura", true},
		{"a senha: hunter22", true},
		{"PASSWORD=correcthorse", true},
		{"token = abcdef123456", true},
		{`"api_key": "abc123def456"`, true},
		{"client_secret=zzzzzzzz", true},
		{"db_password=hunter2222", true},
		{"SENHA_DB=hunter2222", true},
		{"minhasenha: hunter2222", true},
		{"mnosenha: hunter2222", true},
		{"a senha do banco é hunter2222", true},
		{"A senha é: s3gr3d0!", true},
		{"AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG", true},
		{"export GITHUB_TOKEN=abc123def456", true},

		// Falsos positivos comuns.
		{"o token de acesso expirou, renova pra mim?", false},
		{"corrige a tela de senha do login", false},
		{"senha: 123", false},
		{"usa o sk-learn pra isso", false},
		{"uma operação risk-free, sem medo", false},
		{"o campo password precisa de validação", false},
		{"rode git log --oneline e me diga o que mudou", false},
		{"a senha é obrigatória no cadastro", false},
		{"a senha do usuário é validada no servidor", false},
		{"chave_primaria: id", false},
		{"token_expira_em: amanhã", false},
		{"| senha | texto |", false},
		{"", false},
	}
	for _, c := range casos {
		if got := Parece(c.texto); got != c.parece {
			t.Errorf("Parece(%q) = %v, esperado %v", c.texto, got, c.parece)
		}
	}
}
