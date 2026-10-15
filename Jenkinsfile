pipeline {
    agent {
        label 'rust-linux'
    }

    environment {
        SONAR_SERVER = 'SonarQube-Server'
        SCANNER_HOME = tool 'SonarScanner'
        PATH = "/home/jenkins/.cargo/bin:${env.PATH}"
        CARGO_TERM_COLOR = 'always'
        TARGET = 'x86_64-unknown-linux-gnu'
    }

    stages {
        stage('Checkout Source') {
            steps {
                checkout scm
            }
        }

        stage('Format, Check & Clippy') {
            steps {
                sh '''
                    cargo --version
                    rustc --version
                    cargo fmt -- --check
                    cargo check --locked --target "$TARGET"
                    cargo clippy --locked --target "$TARGET" -- -D warnings
                '''
            }
        }

        stage('Unit Test') {
            steps {
                sh 'cargo test --locked --target "$TARGET"'
            }
        }

        stage('Build Linux Release') {
            steps {
                sh 'cargo build --locked --release --target "$TARGET"'
            }
        }

        stage('SonarQube Analysis') {
            steps {
                withSonarQubeEnv("${SONAR_SERVER}") {
                    sh "${SCANNER_HOME}/bin/sonar-scanner"
                }
            }
        }

        stage('Quality Gate Check') {
            steps {
                timeout(time: 5, unit: 'MINUTES') {
                    waitForQualityGate abortPipeline: true
                }
            }
        }
    }

    post {
        success {
            archiveArtifacts artifacts: "target/${TARGET}/release/HealthyReminders", fingerprint: true
        }
        always {
            cleanWs()
        }
    }
}
