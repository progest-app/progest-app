using FluentAssertions;
using Xunit;
using Progest.Core.Models.NamingConvention;

namespace Progest.Core.Tests.Models.NamingConvention;

public class SuffixRuleTests
{
    [Fact]
    public void Constructor_WithNoneType_ShouldCreateRule()
    {
        // Arrange & Act
        var rule = new SuffixRule(SuffixType.None);

        // Assert
        rule.Type.Should().Be(SuffixType.None);
    }

    [Fact]
    public void Constructor_WithFixedType_ShouldCreateRule()
    {
        // Arrange & Act
        var rule = new SuffixRule(SuffixType.Fixed, "_v1");

        // Assert
        rule.Type.Should().Be(SuffixType.Fixed);
        rule.FixedValue.Should().Be("_v1");
    }

    [Fact]
    public void Constructor_WithVersionType_ShouldCreateRule()
    {
        // Arrange & Act
        var rule = new SuffixRule(SuffixType.Version, VersionFormat.Semantic, 1, 0, 0);

        // Assert
        rule.Type.Should().Be(SuffixType.Version);
        rule.VersionFormat.Should().Be(VersionFormat.Semantic);
        rule.MajorVersion.Should().Be(1);
        rule.MinorVersion.Should().Be(0);
        rule.PatchVersion.Should().Be(0);
    }

    [Fact]
    public void GenerateSuffix_WithNoneType_ShouldReturnEmpty()
    {
        // Arrange
        var rule = new SuffixRule(SuffixType.None);

        // Act
        var result = rule.GenerateSuffix();

        // Assert
        result.Should().BeEmpty();
    }

    [Fact]
    public void GenerateSuffix_WithFixedType_ShouldReturnFixedValue()
    {
        // Arrange
        var rule = new SuffixRule(SuffixType.Fixed, "_final");

        // Act
        var result = rule.GenerateSuffix();

        // Assert
        result.Should().Be("_final");
    }

    [Fact]
    public void GenerateSuffix_WithSemanticVersion_ShouldReturnVersion()
    {
        // Arrange
        var rule = new SuffixRule(SuffixType.Version, VersionFormat.Semantic, 2, 1, 3);

        // Act
        var result = rule.GenerateSuffix();

        // Assert
        result.Should().Be("v2.1.3");
    }

    [Fact]
    public void GenerateSuffix_WithSimpleVersion_ShouldReturnVersion()
    {
        // Arrange
        var rule = new SuffixRule(SuffixType.Version, VersionFormat.Simple, 3, 5);

        // Act
        var result = rule.GenerateSuffix();

        // Assert
        result.Should().Be("v3.5");
    }

    [Fact]
    public void GenerateSuffix_WithSequentialVersion_ShouldReturnVersion()
    {
        // Arrange
        var rule = new SuffixRule(SuffixType.Version, VersionFormat.Sequential, 42);

        // Act
        var result = rule.GenerateSuffix();

        // Assert
        result.Should().Be("042");
    }

    [Fact]
    public void GenerateSuffix_WithSequentialVersion_PadsWithZeros()
    {
        // Arrange
        var rule = new SuffixRule(SuffixType.Version, VersionFormat.Sequential, 7);

        // Act
        var result = rule.GenerateSuffix();

        // Assert
        result.Should().Be("007");
    }
}
