using FluentAssertions;
using Xunit;
using Progest.Core.Models.NamingConvention;

namespace Progest.Core.Tests.Models.NamingConvention;

public class SequentialNumberingRuleTests
{
    [Fact]
    public void Constructor_ShouldCreateRule()
    {
        // Arrange & Act
        var rule = new SequentialNumberingRule(startNumber: 1, digitCount: 3, separator: "_");

        // Assert
        rule.StartNumber.Should().Be(1);
        rule.DigitCount.Should().Be(3);
        rule.Separator.Should().Be("_");
    }

    [Fact]
    public void Format_WithDefaultParameters_ShouldFormatCorrectly()
    {
        // Arrange
        var rule = new SequentialNumberingRule();
        var number = 5;

        // Act
        var result = rule.Format(number);

        // Assert
        result.Should().Be("0005_");
    }

    [Fact]
    public void Format_WithCustomStart_ShouldFormatCorrectly()
    {
        // Arrange
        var rule = new SequentialNumberingRule(startNumber: 10);
        var number = 15;

        // Act
        var result = rule.Format(number);

        // Assert
        result.Should().Be("0015_");
    }

    [Fact]
    public void Format_WithCustomDigitCount_ShouldPadCorrectly()
    {
        // Arrange
        var rule = new SequentialNumberingRule(digitCount: 5);
        var number = 42;

        // Act
        var result = rule.Format(number);

        // Assert
        result.Should().Be("00042_");
    }

    [Fact]
    public void Format_WithCustomSeparator_ShouldUseSeparator()
    {
        // Arrange
        var rule = new SequentialNumberingRule(separator: "-");
        var number = 123;

        // Act
        var result = rule.Format(number);

        // Assert
        result.Should().Be("0123-");
    }

    [Fact]
    public void Format_WithEmptySeparator_ShouldNotAddSeparator()
    {
        // Arrange
        var rule = new SequentialNumberingRule(separator: "");
        var number = 7;

        // Act
        var result = rule.Format(number);

        // Assert
        result.Should().Be("0007");
    }

    [Fact]
    public void Format_WithLargeNumber_ShouldFormatCorrectly()
    {
        // Arrange
        var rule = new SequentialNumberingRule(digitCount: 4);
        var number = 9999;

        // Act
        var result = rule.Format(number);

        // Assert
        result.Should().Be("9999_");
    }

    [Fact]
    public void Format_WithNumberExceedingDigits_ShouldNotTruncate()
    {
        // Arrange
        var rule = new SequentialNumberingRule(digitCount: 2);
        var number = 123;

        // Act
        var result = rule.Format(number);

        // Assert
        result.Should().Be("123_");
    }

    [Fact]
    public void Format_WithZero_ShouldFormatCorrectly()
    {
        // Arrange
        var rule = new SequentialNumberingRule();
        var number = 0;

        // Act
        var result = rule.Format(number);

        // Assert
        result.Should().Be("0000_");
    }
}
