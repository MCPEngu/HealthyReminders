pipeline {
    agent {
        label 'rust-linux'
    }

    environment {
        SONAR_SERVER = 'SonarQube-Server'
        SCANNER_HOME = tool 'SonarScanner'
        PATH = "/home/jenkins/.cargo/bin:${env.PATH}"
        CARGO_TERM_COLOR = 'always'
        LINUX_TARGET = 'x86_64-unknown-linux-gnu'
        WIN_TARGET = 'x86_64-pc-windows-msvc'
    }

    stages {
        stage('Checkout Source') {
            steps {
                checkout scm
            }
        }

        stage('Format & Lint Check (Linux)') {
            steps {
                sh '''
                    cargo fmt -- --check
                    cargo check --locked --target "$LINUX_TARGET"
                    cargo clippy --locked --target "$LINUX_TARGET" -- -D warnings
                '''
            }
        }

        stage('Lint Check (Windows)') {
            steps {
                sh '''
                    cargo xwin check --locked --target "$WIN_TARGET"
                    cargo xwin clippy --locked --target "$WIN_TARGET" -- -D warnings
                '''
            }
        }

        stage('Unit Test (Linux)') {
            steps {
                sh 'cargo test --locked --target "$LINUX_TARGET"'
            }
        }

        stage('Unit Test (Windows)') {
            steps {
                sh 'cargo xwin test --locked --target "$WIN_TARGET" --no-run'
            }
        }

        stage('Build Release Artifacts') {
            steps {
                sh '''
                    mkdir -p dist

                    echo "=== Building Linux Release ==="
                    cargo build --locked --release --target "$LINUX_TARGET"
                    cp "target/${LINUX_TARGET}/release/HealthyReminders" "dist/HealthyReminders-linux-x64"
                    chmod +x "dist/HealthyReminders-linux-x64"

                    echo "=== Building Windows Release ==="
                    cargo xwin build --locked --release --target "$WIN_TARGET"
                    cp "target/${WIN_TARGET}/release/HealthyReminders.exe" "dist/HealthyReminders-windows-x64.exe"
                '''
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
            archiveArtifacts artifacts: 'dist/*', fingerprint: true
        }
        always {
            cleanWs()
        }
    }
}