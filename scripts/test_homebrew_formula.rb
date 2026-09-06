# frozen_string_literal: true

require "minitest/autorun"
require "fileutils"
require "open3"
require "rbconfig"
require "tmpdir"
require "yaml"

class HomebrewFormulaTemplateTest < Minitest::Test
  RELEASE = File.expand_path("../.github/workflows/release.yml", __dir__)
  CI = File.expand_path("../.github/workflows/ci.yml", __dir__)

  # HBT12.TEMPLATE.4: execute the actual writer, not a copied formula template.
  def test_writer_omits_redundant_version
    render_versions do |formula, _version, _sha|
      refute_match(/^[[:space:]]*version[[:space:]]+/, formula)
    end
  end

  def test_writer_orders_build_dependency_before_macos
    render_versions do |formula, _version, _sha|
      assert_includes formula, 'depends_on "rust" => :build'
      assert_includes formula, "depends_on :macos"
      assert_operator formula.index('depends_on "rust"'), :<, formula.index("depends_on :macos")
    end
  end

  # HBT12.CHAIN.6: two writes in the same directory must fully replace the artifact.
  def test_each_render_preserves_the_exact_remaining_payload
    render_versions do |formula, version, sha|
      expected = <<~RUBY
        class Axterminator < Formula
          desc "Background-first macOS GUI automation with MCP server support"
          homepage "https://github.com/MikkoParkkola/axterminator"
          url "https://github.com/MikkoParkkola/axterminator/archive/refs/tags/v#{version}.tar.gz"
          sha256 "#{sha}"
          license :cannot_represent

          depends_on "rust" => :build
          depends_on :macos

          def install
            system "cargo", "install",
              "--locked",
              "--features", "cli",
              "--root", prefix,
              "--path", "."
          end

          test do
            assert_match "accessibility", shell_output("\#{bin}/axterminator check")
          end
        end
      RUBY
      assert_equal expected, formula, "render #{version}"
    end
  end

  def test_missing_writer_cannot_become_an_empty_success
    assert_raises(Minitest::Assertion) { extract_writer("echo no-writer\n") }
  end

  def test_duplicate_writer_cannot_pick_an_arbitrary_template
    body = update_body
    assert_raises(Minitest::Assertion) { extract_writer(body + "\n" + body) }
  end

  def test_ci_invokes_the_template_regression_without_a_rust_build
    job = YAML.load_file(CI).fetch("jobs")["homebrew-template"]
    assert_kind_of Hash, job, "missing lightweight homebrew-template CI job"
    assert_nil job["if"], "template regressions must not be conditionally disabled"
    refute job.key?("continue-on-error")
    steps = job.fetch("steps")
    checkout = steps.find { |step| step.fetch("uses", "").start_with?("actions/checkout@") }
    refute_nil checkout, "template job must check out the candidate"
    commands = steps.map { |step| step["run"] }.compact
    assert_includes commands.flat_map(&:lines).map(&:strip), "ruby scripts/test_homebrew_formula.rb"
    test_step = steps.find { |step| step.fetch("run", "").lines.map(&:strip).include?("ruby scripts/test_homebrew_formula.rb") }
    assert_nil test_step["if"]
    refute test_step.key?("continue-on-error")
    refute_match(/cargo\s+(?:build|check|test)/, commands.join("\n"))
    refute_match(/rust-toolchain|rust-cache/, steps.map { |step| step["uses"] }.compact.join("\n"))
  end

  private

  def update_body
    steps = YAML.load_file(RELEASE).fetch("jobs").fetch("update-homebrew").fetch("steps")
    matches = steps.select { |step| step["name"] == "Update formula" }
    assert_equal 1, matches.length, "expected exactly one Update formula step"
    body = matches.first.fetch("run")
    refute_empty body.strip
    body
  end

  def extract_writer(body)
    matches = body.scan(/^cat > Formula\/axterminator\.rb << FORMULA\n.*?^FORMULA\n/m)
    assert_equal 1, matches.length, "expected exactly one formula writer"
    refute_empty matches.first.strip
    matches.first
  end

  def render_versions
    writer = extract_writer(update_body)
    Dir.mktmpdir("axterminator-formula-test-") do |dir|
      FileUtils.mkdir_p(File.join(dir, "Formula"))
      shell = File.join(dir, "writer.sh")
      File.write(shell, writer)
      [["0.10.2", "1" * 64], ["0.10.3", "2" * 64]].each do |version, sha|
        env = { "VERSION" => version, "SHA256" => sha,
                "URL" => "https://github.com/MikkoParkkola/axterminator/archive/refs/tags/v#{version}.tar.gz" }
        _out, err, status = Open3.capture3(env, "/bin/bash", "--noprofile", "--norc", "-e", shell, chdir: dir)
        assert status.success?, err
        path = File.join(dir, "Formula/axterminator.rb")
        assert File.file?(path), "writer must produce the formula"
        out, err, status = Open3.capture3(RbConfig.ruby, "-c", path)
        assert status.success?, out + err
        yield File.read(path), version, sha
      end
    end
  end
end
