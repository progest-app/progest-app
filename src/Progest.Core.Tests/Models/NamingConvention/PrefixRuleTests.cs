using FluentAssertions;
using Xunit;
using Progest.Core.Models.NamingConvention;

namespace Progest.Core.Tests.Models.NamingConvention;

public class PrefixRuleTests
{
    [Fact]
    public void Constructor_WithNoneType_ShouldCreateRule()
    {
        // Arrange & Act
        var rule = new PrefixRule(PrefixType.None);

        // Assert
        rule.Type.Should().Be(PrefixType.None);
    }

    [Fact]
    public void Constructor_WithFixedType_ShouldCreateRule()
    {
        // Arrange & Act
        var rule = new PrefixRule(PrefixType.Fixed, "test_");

        // Assert
        rule.Type.Should().Be(PrefixType.Fixed);
        rule.FixedValue.Should().Be("test_");
    }

    [Fact]
    public void Constructor_WithDateType_ShouldCreateRule()
    {
        // Arrange & Act
        var rule = new PrefixRule(PrefixType.Date, DateFormat.IsoDate);

        // Assert
        rule.Type.Should().Be(PrefixType.Date);
        rule.DateFormat.Should().Be(DateFormat.IsoDate);
    }

    [Fact]
    public void GeneratePrefix_WithNoneType_ShouldReturnEmpty()
    {
        // Arrange
        var rule = new PrefixRule(PrefixType.None);

        // Act
        var result = rule.GeneratePrefix();

        // Assert
        result.Should().BeEmpty();
    }

    [Fact]
    public void GeneratePrefix_WithFixedType_ShouldReturnFixedValue()
    {
        // Arrange
        var rule = new PrefixRule(PrefixType.Fixed, "prefix_");

        // Act
        var result = rule.GeneratePrefix();

        // Assert
        result.Should().Be("prefix_");
    }

    [Fact]
    public void GeneratePrefix_WithIsoDateFormat_ShouldReturnDate()
    {
        // Arrange
        var rule = new PrefixRule(PrefixType.Date, DateFormat.IsoDate);
        var expectedDate = DateTime.Now.ToString("yyyyMMdd");

        // Act
        var result = rule.GeneratePrefix();

        // Assert
        result.Should().Be(expectedDate);
    }

    [Fact]
    public void GeneratePrefix_WithIsoDateTimeFormat_ShouldReturnDateTime()
    {
        // Arrange
        var rule = new PrefixRule(PrefixType.Date, DateFormat.IsoDateTime);
        var expectedPattern = "\\d{8}_\\d{4}"; // yyyyMMdd_HHmm

        // Act
        var result = rule.GeneratePrefix();

        // Assert
        result.Should().MatchRegex(expectedPattern);
    }

    [Fact]
    public void GeneratePrefix_WithReverseDateFormat_ShouldReturnDate()
    {
        // Arrange
        var rule = new PrefixRule(PrefixType.Date, DateFormat.ReverseDate);
        var expectedPattern = "\\d{4}-\\d{2}-\\d{2}"; // yyyy-MM-dd

        // Act
        var result = rule.GeneratePrefix();

        // Assert
        result.Should().MatchRegex(expectedPattern);
    }

    [Fact]
    public void GeneratePrefix_WithShortDateFormat_ShouldReturnShortDate()
    {
        // Arrange
        var rule = new PrefixRule(PrefixType.Date, DateFormat.ShortDate);
        var expectedPattern = "\\d{6}"; // yyMMdd

        // Act
        var result = rule.GeneratePrefix();

        // Assert
        result.Should().MatchRegex(expectedPattern);
    }
}
